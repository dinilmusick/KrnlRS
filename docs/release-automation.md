# KrnlRS — automated Rust registry releases

Source of truth: `.github/workflows/release-rust.yml`; `main` changes and pull requests run Cargo CI plus six offline trusted-publisher contract tests. Builds must pass before any publication.

## One-time unattended bootstrap (GitHub-hosted only)

New crate names cannot be published using crates.io OpenID Connect (OIDC). The crate owner must provision one short-lived crates.io API token **once** in the protected GitHub Actions environment `crates-bootstrap` as secret `CRATES_IO_BOOTSTRAP_TOKEN`.

The token needs only the required new-crate publish permission **and** the `Trusted Publishing` endpoint scope; a valid, verified crates.io account owning the crate and linked to the GitHub owner is required. Restrict the environment to `main`, preferably with an approval gate. Do not put this token on developer machines or in source.

Set repository variable `CRATES_IO_BOOTSTRAP_READY=true`, then dispatch **Rust crates.io release** on `main` with input `bootstrap=true`. The workflow checks the exact main SHA, runs tests and Cargo package validation, and uses release-plz to publish the first version. Immediately after publication, `scripts/register_trusted_publisher.py`:

- Confirms the exact committed name and version exist on crates.io (up to two minutes of indexing wait).
- Uses the scoped API token to list existing trusted publishers; reuses an exact match or registers GitHub owner `dinilmusick`, repository `KrnlRS`, workflow filename `release-rust.yml`, GitHub environment `crates-io`.
- Fails closed for mismatched publishers, missing ownership, invalid scope, registry outage or unexpected API response, without logging the token.

Check the crates.io crate page and exact publisher config; disable `CRATES_IO_BOOTSTRAP_READY`, remove/revoke the bootstrap secret/token. Set `CRATES_IO_TRUSTED_PUBLISHING_READY=true` **only after** successful registration and actual registry checks.

## Automatic version bump and release

Configure GitHub environment `crates-io`, restricted to `main`, matching the OIDC Trusted Publisher. The `publish` job obtains a short-lived token through OIDC—**no reusable crates.io token or environment secret**. After a validated merge, release-plz publishes unpublished versions, tags and creates releases.

For automatic version-changelog pull requests with real CI triggers (not suppressed GitHub bot PRs), install a least-privilege GitHub App with **Actions read, Contents write and Pull requests write** permissions. Configure `RELEASE_PLZ_APP_ID` (repository variable), `RELEASE_PLZ_APP_PRIVATE_KEY` (GitHub Actions secret), `RELEASE_PLZ_APP_READY=true` (repository variable). The workflow mints short-lived installation tokens; it never embeds a personal token.

Release PR merges must be subject to required CI checks. Set up branch protection / approved auto-merge policy separately. `release-plz` follows Conventional Commits (`fix:`, `feat:`, `feat!:`). An unpublished version is **not** evidence of a successful registry release.

## Certification

Verify a crate's exact published version and provenance in crates.io; install it in a new Cargo project from the **registry** without Git overrides, run tests, confirm GitHub tag/source SHA and post-merge CI. A green workflow with `publish` skipped is not publication.

There are no source-code credentials. Missing GitHub Actions secrets, variables, environment permissions, or crates.io ownership require the account owner to authorize setup once; these cannot be inferred from CI status.
