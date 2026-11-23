# Automated Rust releases

On every push to `main`, GitHub Actions tests and packages this crate.
`release-plz` prepares a version/changelog release pull request; merging that
pull request triggers release publication via crates.io OIDC Trusted Publishing.
No local credential or long-lived crates.io token is used for routine releases.

## One-time external prerequisites

1. Publish **the first crate version** to crates.io from GitHub Actions with a
   scoped bootstrap token stored only in GitHub Secrets (or use an existing
   crates.io release). crates.io does not support OIDC for the first version.
2. At crates.io configure a Trusted Publisher matching this GitHub
   repository, workflow `release-rust.yml`, environment `crates-io`.
3. In GitHub create environment `crates-io`, restricting deployment to
   `main`, and set repository variable
   `CRATES_IO_TRUSTED_PUBLISHING_READY=true` only after trusted-publisher
   registration succeeds. Until then publishing is explicitly disabled.
4. Allow GitHub Actions to create release PRs, and use a GitHub App token
   (instead of the default GITHUB_TOKEN) if release PR checks must trigger
   automatically. Default GITHUB_TOKEN-created PRs do not trigger CI.

Use Conventional Commits (`fix:`, `feat:`, `feat!:`) for release versions.
This release workflow does not bypass existing PR checks, branch protection,
or registry ownership. Do not automatically merge generated release PRs
without independent test results. Published crate versions are immutable.
