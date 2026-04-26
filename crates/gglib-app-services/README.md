# gglib-app-services

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-complexity.json)

Shared GUI backend facade for gglib adapters (Tauri desktop, Axum web).

## Architecture

This crate is a **Shared Facade** — sitting between adapters and infrastructure, providing a unified orchestration layer.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 Adapter Layer                                       │
│            ┌────────────────────┐         ┌────────────────────┐                    │
│            │    gglib-tauri     │         │     gglib-axum     │                    │
│            │  (Desktop IPC)     │         │    (HTTP API)      │                    │
│            └─────────┬──────────┘         └─────────┬──────────┘                    │
│                      │                              │                               │
│                      └──────────────┬───────────────┘                               │
│                                     ▼                                               │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                         ►►► gglib-app-services ◄◄◄                                   │   │
│   │         Platform-agnostic GUI orchestration (ensures feature parity)        │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                     │                                               │
└─────────────────────────────────────┼───────────────────────────────────────────────┘
                                      ▼
              ┌───────────────────────────────────────────────────────┐
              │     gglib-core, gglib-db, gglib-runtime, etc.         │
              │              (Infrastructure crates)                   │
              └───────────────────────────────────────────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-app-services                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │  backend    │  │    deps     │  │   error     │  │   types     │                 │
│  │ GuiBackend  │  │  GuiDeps    │  │  GuiError   │  │  Shared DTOs│                 │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                 │
│         │                │                │                │                        │
│         ▼                ▼                ▼                ▼                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │  downloads  │  │   models    │  │   servers   │  │  settings   │                 │
│  │DownloadOps  │  │  ModelOps   │  │ ServerOps   │  │ SettingsOps │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│              ┌─────────────┐   ┌─────────────┐                                      │
│              │     mcp     │   │    voice    │                                      │
│              │   McpOps    │   │  VoiceOps   │                                      │
│              └─────────────┘   └─────────────┘                                      │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`backend.rs`](src/backend.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-backend-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-backend-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-backend-coverage.json) |
| [`deps.rs`](src/deps.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-deps-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-deps-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-deps-coverage.json) |
| [`downloads.rs`](src/downloads.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-coverage.json) |
| [`mcp.rs`](src/mcp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-coverage.json) |
| [`proxy.rs`](src/proxy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-coverage.json) |
| [`servers.rs`](src/servers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-coverage.json) |
| [`settings.rs`](src/settings.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-coverage.json) |
| [`setup.rs`](src/setup.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-coverage.json) |
| [`types.rs`](src/types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-coverage.json) |
| [`voice.rs`](src/voice.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-voice-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-voice-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-voice-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`backend.rs`** — `GuiBackend` main facade with all GUI operations
- **`deps.rs`** — `GuiDeps` dependency injection struct for construction
- **`error.rs`** — `GuiError` semantic error type for GUI operations
- **`downloads.rs`** — `DownloadOps` download queue and progress operations
- **`models.rs`** — `ModelOps` model CRUD and listing operations
- **`servers.rs`** — `ServerOps` llama.cpp server lifecycle management
- **`settings.rs`** — `SettingsOps` application settings persistence
- **`mcp.rs`** — `McpOps` MCP server configuration and management
- **`types.rs`** — Shared DTOs and type definitions for GUI layer

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, etc.
2. **Pure Orchestration** — All deps injected via `GuiDeps`
3. **Trait-Based Injection** — Uses port traits, not concrete impls
4. **Semantic Errors** — Returns `GuiError`, adapters map to their error types
5. **Feature Parity** — Ensures desktop and web UIs have identical capabilities

## Usage

```rust
use gglib_gui::{GuiBackend, GuiDeps};
use std::sync::Arc;

# // This example shows the typical usage pattern.
# // In practice, dependencies would be injected from main or a factory.
# fn example(
#     core: Arc<gglib_core::services::AppCore>,
#     downloads: Arc<dyn gglib_core::ports::DownloadManagerPort>,
#     hf: Arc<dyn gglib_core::ports::HfClientPort>,
#     runner: Arc<dyn gglib_core::ports::ProcessRunner>,
#     mcp: Arc<gglib_mcp::McpService>,
#     emitter: Arc<dyn gglib_core::ports::AppEventEmitter>,
#     server_events: Arc<dyn gglib_core::events::ServerEvents>,
#     tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort>,
#     proxy_supervisor: Arc<gglib_runtime::proxy::ProxySupervisor>,
#     model_repo: Arc<dyn gglib_core::ports::ModelRepository>,
#     system_probe: Arc<dyn gglib_core::ports::SystemProbePort>,
#     gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort>,
# ) {
// Construct backend with dependency injection
let deps = GuiDeps::new(
    core,
    downloads,
    hf,
    runner,
    mcp,
    emitter,
    server_events,
    tool_detector,
    proxy_supervisor,
    model_repo,
    system_probe,
    gguf_parser,
);

let backend = GuiBackend::new(deps);

// Use backend operations (async examples shown in comments)
// let models = backend.list_models().await?;
// let queue = backend.get_download_queue().await;
// backend.start_server(model_id, request).await?;
# }
```
