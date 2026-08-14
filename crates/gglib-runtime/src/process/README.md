# process

<!-- module-docs:start -->

Process management infrastructure for GUI applications.

This module provides shared infrastructure for managing llama-server processes
with integrated log streaming and event broadcasting for GUI use cases.

# Structure

- `GuiProcessCore` - Low-level process spawning with log streaming (u32 model IDs)
- `ProcessManager` - High-level orchestration; dispatch only
- `ResidentSet` (`residency/`) - The VRAM slots and the launch driver that fills them
- `AdmissionQueue` (`admission/`) - Decides who gets the GPU next, and for how long
- `ServerEvent` / `ServerEventBroadcaster` - Lifecycle event broadcasting
- `ServerLogManager` - Log streaming infrastructure
- Health check utilities

# Residency and admission

`ProcessManager` routes; [`ResidentSet`] (`residency/`) owns the state. M9
replaced the old single-swap strategy with a bounded resident set — a small
number of VRAM slots filled by an `AdmissionQueue` (`admission/`) that decides,
per request: serve from a resident model, launch into a free or evictable
slot, wait, or give up.

A set may be *pinned*: `ProcessManager::set_pin` restricts admission to
exactly one model (backing `gglib serve`) and returns `PinnedModelMismatch`
for any other, rather than swapping. Pinning changes only which models are
admitted — startup coordination, cache handling and launch options are
identical either way.

Every launch surface — the CLI, the proxy, both GUIs — shares one manager
built by `build_service_graph` inside the daemon, which is what makes "gglib
owns every llama-server on this machine" an invariant. A `Concurrent`
strategy existed here for the GUI's earlier direct-spawn path; epic #630
routed the GUI through the proxy's manager instead, so it was deleted along
with the rest of that path.

# Launch options

The `ResidentSet` carries a standing `ServerConfigOptions` template rather
than a hand-picked list of fields. Each launch resolves to

```text
template  ⊕  per-call overrides  ⊕  this request's context chain
```

so a flag added to `ServerConfigOptions` reaches llama-server through this path
with no change here at all.

# On the `Gui` prefix

`GuiProcessCore` was named against a second, port-aligned `ProcessCore` in
`process_core.rs`, which implemented a `ProcessRunner` trait for CLI use with
no log or event infrastructure. `process_core.rs` went in #708; the trait
outlived it and went in #849, once it had no implementor left. This is now the
only process core and serves every caller — the prefix is vestigial.

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
| [`stream.rs`](stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-stream-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-types-coverage.json) |
| [`admission/`](admission/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-admission-coverage.json) |
| [`residency/`](residency/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-residency-coverage.json) |
| [`shutdown/`](shutdown/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-coverage.json) |
<!-- module-table:end -->

</details>
