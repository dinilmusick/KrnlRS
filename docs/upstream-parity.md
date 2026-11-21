# KrnlRS / pinned KrnlTS contract parity

Authority: `dinilmusick/KrnlTS@9fdd0eb792361dd72b57cb678b933ba0c8fcae3e`.
Native foundation: `dinilmusick/GutRS@3e4ad1823166b37ddc437aab04277b42c124ce36`.
Behavioral ownership: KrnlRS executes and orchestrates functionality **inside one service**; cross-service orchestration belongs to Skipo.

| KrnlTS public contract | KrnlRS implementation | Executable verification |
|---|---|---|
| `LifecycleManager` seven states/rollback | `lifecycle::Lifecycle` from PR #1 | `tests/upstream_golden.rs` exhaustive 49 source/target transitions |
| Service execution + restart/terminal disposal | `Service<F>` PR #4 | unit lifecycle, serialized concurrency and failure tests |
| Service construction + capabilities + dependencies | `registry::ServiceDescriptor`, `ServiceRegistry` | registry unit tests |
| Functionality boundary & ownership | `Functionality`, `ownership` from PR #3 | ownership and invocation tests |
| DAG orchestration / dependency validation | `orchestration::FunctionalityOrchestration` | 13 DAG tests in upstream PR #4 |
| Service-owned typed state | `state::ServiceState` | state snapshot and mutation test |
| Service-owned events | `events::ServiceEvents` | topic delivery and subscriber cleanup test |
| Protocol-neutral proxies | `protocol::ServiceProxy` / `ServiceTransport` | host adapter propagation test |
| Native contract validation and diagnostics | `validation::validate_manifest`, `krnl validate` | validator tests |
| Opt-in bounded telemetry | `telemetry::Telemetry` from PR #2 | telemetry tests |
| KrnlQ concurrency/timeout/retry | `queue::KrnlQueue` | queue tests |
| External consumer compile | `examples/consumer.rs` | `cargo check --example consumer` |
| Public package boundary | `src/lib.rs` | `cargo test --all-targets` and Clippy |

**Intentional adapters:** HTTP, WebSocket, UDP, platform clocks and external networking are provided by hosts through `ServiceTransport`. There are no runtime dependencies on Kloks, Floks, Skipo or Krusbl.

**Limitations:** The synchronous queue uses cooperative deadlines: it cannot forcibly interrupt a user callback that ignores its cancellation/deadline context. Validation CLI uses a deterministic portable manifest DSL rather than mirroring TypeScript AST parsing. `std` is required by the current synchronization layer; an `alloc`-only foundation is future work. The GutRS external crate is not required at runtime; the pinned portable foundation is verified independently.
