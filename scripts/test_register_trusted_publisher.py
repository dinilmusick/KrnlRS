"""Offline safety tests for the crates.io Trusted Publisher bootstrap."""
import os
import unittest
from unittest.mock import patch
import register_trusted_publisher as mod


class BootstrapTests(unittest.TestCase):
    def test_expected_identity(self):
        self.assertEqual(mod.publisher_record("skipo-rs", "dinilmusick/SkipoRS"),
                         {"crate": "skipo-rs", "repository_owner": "dinilmusick",
                          "repository_name": "SkipoRS", "workflow_filename": "release-rust.yml",
                          "environment": "crates-io"})

    def test_existing_publisher_is_idempotent(self):
        record = mod.publisher_record("krnlrs", "dinilmusick/KrnlRS")
        with patch.object(mod, "request_json", return_value=(200, {"github_configs": [record]})) as api:
            mod.ensure_publisher("krnlrs", "fake", "dinilmusick/KrnlRS")
            api.assert_called_once()

    def test_new_publisher_matches_exact_details(self):
        record = mod.publisher_record("gutrs", "dinilmusick/GutRS")
        with patch.object(mod, "request_json", side_effect=[
            (200, {"github_configs": []}), (200, {"github_config": record})]) as api:
            mod.ensure_publisher("gutrs", "fake", "dinilmusick/GutRS")
            self.assertEqual(api.call_count, 2)
            self.assertEqual(api.call_args.kwargs["payload"], {"github_config": record} if "payload" in api.call_args.kwargs else api.call_args.args[2])

    def test_conflicting_environment_fails_closed(self):
        record = mod.publisher_record("frnlrs", "dinilmusick/FrnlRS")
        record["environment"] = "incorrect"
        with patch.object(mod, "request_json", return_value=(200, {"github_configs": [record]})):
            with self.assertRaisesRegex(RuntimeError, "different environment"):
                mod.ensure_publisher("frnlrs", "fake", "dinilmusick/FrnlRS")

    def test_registry_version_must_match(self):
        with patch.object(mod, "request_json", return_value=(200, {"version": {"num": "0.2.0"}})):
            with self.assertRaisesRegex(RuntimeError, "mismatched"):
                mod.assert_published("gutrs", "0.1.0")

    def test_untrusted_workflow_ref_is_blocked(self):
        with patch.dict(os.environ, {
            "CARGO_REGISTRY_TOKEN": "fake",
            "GITHUB_REPOSITORY": "dinilmusick/GutRS",
            "GITHUB_REF": "refs/heads/feature",
            "GITHUB_WORKFLOW_REF": "dinilmusick/GutRS/.github/workflows/release-rust.yml@refs/heads/main"
        }):
            with self.assertRaisesRegex(RuntimeError, "Only main-branch"):
                mod.main()


if __name__ == "__main__":
    unittest.main()
