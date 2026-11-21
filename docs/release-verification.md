# KrnlRS Wave 2 acceptance gates

Source of behavior: KrnlTS `9fdd0eb792361dd72b57cb678b933ba0c8fcae3e`; GutRS `3e4ad1823166b37ddc437aab04277b42c124ce36`.

This document records the executable verification contract rather than a claim that a particular commit has passed. Inspect exact SHA and run conclusions before merging.

- `cargo fmt --all -- --check`: canonical Rust formatting.
- `cargo clippy --all-targets -- -D warnings`: error-free static lint across library, executable, examples and tests.
- `cargo test --all-targets`: lifecycle, queue concurrency, failure paths, registry, typed facade, telemetry, state, event, proxy and golden fixtures.
- `cargo test --doc`: public Rust documentation samples.
- `cargo check --no-default-features --lib`: portable `no_std` / `alloc` foundation.
- `cargo check --no-default-features --example portable_consumer` and `cargo run --no-default-features --example portable_consumer`: portable external consumer.
- `cargo check --example consumer` and `cargo run --example consumer`: external std runtime including facade and host transport.
- `cargo run --bin krnl -- validate testdata/manifests/valid.krnl`: valid manifest accepted.
- `cargo run --bin krnl -- validate testdata/manifests/invalid.krnl`: invalid manifest rejected with nonzero exit.

The final integration gate also requires live GitHub check runs on the exact PR head, expected-head guarded merge, and independent main-branch readback. Workflow `action_required` is not a test pass.
