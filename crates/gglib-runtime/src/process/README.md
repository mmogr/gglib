# process

<!-- module-docs:start -->

Process management infrastructure for GUI applications.

This module provides shared infrastructure for managing llama-server processes
with integrated log streaming and event broadcasting for GUI use cases.

# Structure

- `GuiProcessCore` - Low-level process spawning with log streaming (u32 model IDs)
- `ProcessManager` - High-level orchestration; dispatch only
- `SwapState` - Single-model-at-a-time state and the startup driver itself
- `ServerEvent` / `ServerEventBroadcaster` - Lifecycle event broadcasting
- `ServerLogManager` - Log streaming infrastructure
- Health check utilities

# Strategies

`ProcessStrategy` has one shape today:

- **SingleSwap** — one model at a time, holding a [`SwapState`]. Optionally
  *pinned*: `ProcessManager::new_pinned` serves exactly one model and returns
  `PinnedModelMismatch` for any other, rather than swapping. Pinning changes
  only which models are admitted — startup coordination, cache handling and
  launch options are identical either way.

Every launch surface — the CLI, the proxy, both GUIs — shares one
`SingleSwap` manager built by `build_service_graph`, which is what makes
"only one llama-server runs at a time system-wide" an invariant. A
`Concurrent` strategy existed here for the GUI's earlier direct-spawn path;
epic #630 routed the GUI through the proxy's manager instead, so it was
deleted along with the rest of that path.

# Launch options

`SwapState` carries a standing `ServerConfigOptions` template rather than a
hand-picked list of fields. Each launch resolves to

```text
template  ⊕  per-call overrides  ⊕  this request's context chain
```

so a flag added to `ServerConfigOptions` reaches llama-server through this path
with no change here at all.

# Distinction from `ProcessCore`

This module's `GuiProcessCore` is distinct from the port-aligned `ProcessCore`
in `process_core.rs`. The port version implements `ProcessRunner` for CLI use
with `i64` model IDs and no log/event infrastructure.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`broadcaster.rs`](broadcaster.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-broadcaster-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-broadcaster-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-broadcaster-coverage.json) |
| [`core.rs`](core.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-core-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-core-coverage.json) |
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-events-coverage.json) |
| [`health.rs`](health.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-health-coverage.json) |
| [`logs.rs`](logs.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-logs-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-logs-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-logs-coverage.json) |
| [`manager.rs`](manager.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-manager-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-manager-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-manager-coverage.json) |
| [`ports.rs`](ports.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-ports-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-ports-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-ports-coverage.json) |
| [`startup_guard.rs`](startup_guard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard-coverage.json) |
| [`startup_guard_tests.rs`](startup_guard_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-startup_guard_tests-coverage.json) |
| [`stream.rs`](stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-coverage.json) |
| [`swap_state.rs`](swap_state.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-swap_state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-swap_state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-swap_state-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-coverage.json) |
| [`shutdown/`](shutdown/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-coverage.json) |
<!-- module-table:end -->

</details>
