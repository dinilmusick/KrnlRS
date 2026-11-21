# KrnlRS

Portable, Rust-native service kernel. Canonical upstream contracts: [KrnlTS](https://github.com/dinilmusick/KrnlTS) at `9fdd0eb792361dd72b57cb678b933ba0c8fcae3e`.

## Service ownership

KrnlRS owns **one service** and its typed functionalities, local functionality DAG, lifecycle, validation, execution queue, state, events, telemetry and protocol-neutral adapters. Skipo owns cross-service orchestration. No Kloks, Floks, Skipo or Krusbl runtime dependency.

## Build and verify

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --doc
cargo check --example consumer
cargo check --no-default-features --lib
cargo check --no-default-features --example portable_consumer
cargo run --bin krnl -- validate testdata/manifests/valid.krnl
```

## Runtime use

`facade::ServiceFacade` joins seven-state lifecycle hooks, bounded queue admission, checked typed inputs and outputs, correlated invocation telemetry, restartable stop, and terminal dispose. `queue::ExecutionContext` provides cooperative cancellation and timeouts, with structured retry and failure semantics. Callbacks may be retried; the caller must ensure idempotence.

The host supplies networking using `protocol::ServiceTransport`; the kernel does not open sockets or require a resident worker.

`registry::ServiceRegistry` resolves service dependencies deterministically and discovers capabilities, while `ownership` and `orchestration` restrict functionality composition to one service.

The `portable` module supports `no_std` + `alloc` with the default `std` feature disabled. It includes core lifecycle and identifier/operation contracts. Concurrency, network transports and CLI are deliberately `std`-only.

## CLI manifest

```text
service service-name 1.0
capability service-name can-read
depends service-name other-service
```

Comments start with `#`. `krnl validate <file>` exits nonzero and prints stable line/code diagnostics for malformed definitions, missing dependencies, cycles, duplicates and invalid identifiers.

See [upstream parity](docs/upstream-parity.md) for contract map and executed CI gates.
