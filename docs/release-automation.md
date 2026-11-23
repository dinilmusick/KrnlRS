# KrnlRS — verifiable crates.io release operations

The single workflow `.github/workflows/release-rust.yml` has four jobs.
Every pull request and push to main runs portable Cargo tests.
Cargo registry dry-run/package validation also runs on every pull request and push.

## Initial publication (one time per new crate name)

1. Verify the crate name is owned or available on crates.io; verify the committed Cargo version and dependencies. crates.io does not accept OIDC Trusted Publishing for a crate's first version.
2. Create the GitHub environment `crates-bootstrap`, restricted to the main branch and preferably requiring maintainer approval. Add a **scoped, short-lived crates.io token with publish-new permission** as environment secret `CRATES_IO_BOOTSTRAP_TOKEN`. Never copy it to a local shell, application env, source, logs or task ledger.
3. Set repository variable `CRATES_IO_BOOTSTRAP_READY=true`. From GitHub Actions dispatch `Rust crates.io release` **on main** with input `bootstrap=true`. This uses release-plz under the GitHub runner, with the scoped token, and must pass validation. Check crates.io and the GitHub release/tag separately.
4. Remove and revoke the bootstrap token and set `CRATES_IO_BOOTSTRAP_READY=false` after verifying first publication.

## Routine tokenless publication

1. In crates.io package Settings > Trusted Publishing, register owner `dinilmusick`, repo `KrnlRS`, workflow `release-rust.yml`, GitHub environment `crates-io`. This must be done by a package owner.
2. Configure GitHub environment `crates-io` restricted to main. Only after confirming the exact publisher registration and published initial crate set repository variable `CRATES_IO_TRUSTED_PUBLISHING_READY=true`.
3. On subsequent verified merges to `main`, release-plz `release` uses GitHub OIDC to publish any bumped, unpublished version. The job has `id-token: write`; it deliberately does not read a long-lived registry token.
4. A release PR uses a short-lived **GitHub App installation token** so that its own GitHub PR checks actually run. Install a least-privilege GitHub App with Contents and Pull requests write access in this repository; set `RELEASE_PLZ_APP_ID` as a GitHub variable, `RELEASE_PLZ_APP_PRIVATE_KEY` as a GitHub secret, and `RELEASE_PLZ_APP_READY=true` only once configured. The release PR is not automatically merged; enforce required checks, then merge via your approved policy. Do not bypass protections.

The old approach using the built-in `GITHUB_TOKEN` to create release PRs is intentionally avoided: such PRs do not launch required CI, and repositories may explicitly deny PR creation from GitHub Actions. No automatic release is claimed until the registry, environment, App and clean external consumer tests confirm it. Use Conventional Commits (`fix:`, `feat:`, `feat!:`) for semantic version changes.

## Release order and acceptance

The independent foundations (GutRS, KrnlRS, KloksRS, FrnlRS) can be released first; dependent FloksRS and SkipoRS require these versions in the registry before enabling their registry gates.

**Do not claim a release complete** from a green GitHub Action alone. Verify the new version on crates.io, Cargo installation from a clean external project with no git override, and source/tag identity. Keep the GitHub repository and crates.io permissions least-privilege.