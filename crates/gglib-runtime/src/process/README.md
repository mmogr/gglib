# process

<!-- module-docs:start -->

Process management infrastructure for GUI applications.

Provides shared infrastructure for managing llama-server processes with integrated log streaming and event broadcasting for GUI use cases.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              process/                                              │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────────┐                                                               │
│  │ ProcessManager  │  High-level concurrent process orchestration                  │
│  └────────┬────────┘                                                               │
│           │                                                                        │
│           ▼                                                                        │
│  ┌─────────────────┐                                                               │
│  │ GuiProcessCore  │  Low-level process spawning with log streaming               │
│  └────────┬────────┘                                                               │
│           │                                                                        │
│   ┌───────┼───────────────┬─────────────────┐                                      │
│   ▼       ▼              ▼                 ▼                                      │
│ Events  Logs       Health           Shutdown                                       │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Components

| Component | Description |
|-----------|-------------|
| `ProcessManager` | High-level orchestration (start/stop/switch models) |
| `GuiProcessCore` | Low-level spawning with log streaming |
| `ServerEventBroadcaster` | Lifecycle event broadcasting |
| `ServerLogManager` | Log streaming infrastructure |

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
| [`shutdown/`](shutdown/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-coverage.json) |
<!-- module-table:end -->

</details>
