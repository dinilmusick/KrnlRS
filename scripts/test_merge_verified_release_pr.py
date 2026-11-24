"""Fail-closed tests for CI-verified release-plz pull request auto-merge."""
import unittest
from unittest.mock import patch
import merge_verified_release_pr as merger


def sample():
    return {
        "state": "open", "draft": False, "number": 15,
        "base": {"ref": "main", "sha": "b" * 40},
        "head": {"ref": "release-plz-2026-11-24T04-00-00Z", "sha": "a" * 40,
                 "repo": {"full_name": "dinilmusick/KrnlRS"}},
        "user": {"type": "Bot", "login": "ultrst-release[bot]"},
        "body": "This PR was generated with release-plz"
    }


def run(name="KrnlRS contract CI", conclusion="success", event="pull_request"):
    return {"name": name, "conclusion": conclusion, "status": "completed",
            "event": event, "head_sha": "a" * 40,
            "head_branch": "release-plz-2026-11-24T04-00-00Z"}


class ReleaseMergerTests(unittest.TestCase):
    def test_only_exact_bot_generated_release_pr_accepted(self):
        pr = sample()
        self.assertTrue(merger.eligible_pr(pr, "dinilmusick/KrnlRS", "b" * 40))
        pr["head"]["ref"] = "feature/change"
        self.assertFalse(merger.eligible_pr(pr, "dinilmusick/KrnlRS", "b" * 40))

    def test_untrusted_bot_rejected(self):
        pr = sample()
        pr["user"]["type"] = "User"
        self.assertFalse(merger.eligible_pr(pr, "dinilmusick/KrnlRS", "b" * 40))

    def test_changes_only_release_metadata(self):
        self.assertTrue(merger.release_file_allowed("Cargo.toml"))
        self.assertTrue(merger.release_file_allowed("CHANGELOG.md"))
        self.assertFalse(merger.release_file_allowed(".github/workflows/release-rust.yml"))
        self.assertFalse(merger.release_file_allowed("src/lib.rs"))

    def test_missing_pending_duplicate_and_wrong_event_all_block(self):
        self.assertTrue(merger.checks_pass([run()], "a" * 40,
                       sample()["head"]["ref"], ["KrnlRS contract CI"]))
        self.assertFalse(merger.checks_pass([], "a" * 40,
                        sample()["head"]["ref"], ["KrnlRS contract CI"]))
        self.assertFalse(merger.checks_pass([run(conclusion="failure")], "a" * 40,
                        sample()["head"]["ref"], ["KrnlRS contract CI"]))
        self.assertFalse(merger.checks_pass([run(event="push")], "a" * 40,
                        sample()["head"]["ref"], ["KrnlRS contract CI"]))
        self.assertFalse(merger.checks_pass([run(), run()], "a" * 40,
                        sample()["head"]["ref"], ["KrnlRS contract CI"]))

    def test_merge_only_after_exact_checks_and_recheck(self):
        p = sample()
        sha = p["head"]["sha"]
        paths = [
            "/repos/dinilmusick/KrnlRS/git/ref/heads/main",
            "/repos/dinilmusick/KrnlRS/pulls?state=open&base=main&per_page=100",
        ]
        records = [
            {"object": {"sha": "b" * 40}},
            [p], [{"filename": "Cargo.toml"}],
            {"total_count": 1, "workflow_runs": [run()]},
            dict(p, mergeable=True), {"object": {"sha": "b" * 40}},
            {"merged": True}
        ]
        with patch.object(merger, "github", side_effect=records) as api:
            merger.merge_one("dinilmusick/KrnlRS", "fake", ["KrnlRS contract CI"])
            self.assertEqual(api.call_count, 7)
            self.assertEqual(api.call_args.args[2], "PUT")
            self.assertEqual(api.call_args.args[3]["sha"], sha)

    def test_no_merge_if_checks_missing(self):
        p = sample()
        records = [
            {"object": {"sha": "b" * 40}}, [p], [{"filename": "Cargo.toml"}],
            {"total_count": 0, "workflow_runs": []},
        ]
        with patch.object(merger, "github", side_effect=records) as api:
            merger.merge_one("dinilmusick/KrnlRS", "fake", ["KrnlRS contract CI"])
            self.assertEqual(api.call_count, 4)


if __name__ == "__main__":
    unittest.main()
