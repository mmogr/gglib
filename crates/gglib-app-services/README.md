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
│  │  downloads  │  │   models    │  │   servers   │  │  settings   │                 │
│  │ DownloadOps │  │  ModelOps   │  │  ServerOps  │  │ SettingsOps │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │    error    │  │    types    │  │     mcp     │  │    proxy    │                 │
│  │  GuiError   │  │ Shared DTOs │  │   McpOps    │  │  ProxyOps   │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│              ┌─────────────┐                                                         │
│              │    setup    │                                                         │
│              │  SetupOps   │                                                         │
│              └─────────────┘                                                         │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`downloads.rs`](src/downloads.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-downloads-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-error-coverage.json) |
| [`mcp.rs`](src/mcp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-mcp-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-models-coverage.json) |
| [`proxy.rs`](src/proxy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-proxy-coverage.json) |
| [`servers.rs`](src/servers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-servers-coverage.json) |
| [`settings.rs`](src/settings.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-settings-coverage.json) |
| [`setup.rs`](src/setup.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-setup-coverage.json) |
| [`test_support.rs`](src/test_support.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-test_support-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-test_support-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-test_support-coverage.json) |
| [`types.rs`](src/types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-types-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`downloads.rs`** — `DownloadOps` download queue and progress operations
- **`error.rs`** — `GuiError` semantic error type for all app-service operations
- **`mcp.rs`** — `McpOps` MCP server configuration and management
- **`models.rs`** — `ModelOps` model CRUD and listing operations
- **`proxy.rs`** — `ProxyOps` OpenAI-compatible proxy lifecycle management
- **`servers.rs`** — `ServerOps` llama.cpp server lifecycle management
- **`settings.rs`** — `SettingsOps` application settings persistence
- **`setup.rs`** — `SetupOps` first-run setup and dependency checking
- **`types.rs`** — Shared DTOs and type definitions for the service layer

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, etc.
2. **Pure Orchestration** — All deps injected via per-domain `*Deps` structs
3. **Trait-Based Injection** — Uses port traits, not concrete impls
4. **Semantic Errors** — Returns `GuiError`, adapters map to their error types
5. **Feature Parity** — Ensures desktop and web UIs have identical capabilities

## Usage

```rust
use gglib_app_services::{ModelOps, ModelDeps, ServerOps, ServerDeps};
use std::sync::Arc;

# // This example shows the typical usage pattern.
# // In practice, dependencies would be injected from the adapter bootstrap.
# fn example(
#     core: Arc<gglib_core::services::AppCore>,
#     runner: Arc<dyn gglib_core::ports::ProcessRunner>,
#     gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort>,
#     emitter: Arc<dyn gglib_core::ports::AppEventEmitter>,
#     server_events: Arc<dyn gglib_core::events::ServerEvents>,
#     tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort>,
# ) {
// Construct per-domain ops with injected dependencies
let model_ops = ModelOps::new(ModelDeps {
    core: core.clone(),
    runner: runner.clone(),
    gguf_parser,
});

let server_ops = ServerOps::new(ServerDeps {
    core: core.clone(),
    runner,
    emitter,
    server_events,
    tool_detector,
});

// Use ops asynchronously in handlers
// let models = model_ops.list().await?;
// server_ops.start(model_id, request).await?;
# }
```

## Testing

Each ops module has an inline `#[cfg(test)] mod tests` block.  Tests run against an
in-memory SQLite database provisioned by `gglib_db::setup_test_database()` and
`CoreFactory::build_app_core()`.  All external dependencies are replaced by
handwritten mock structs in `src/test_support.rs` (no external mocking framework).

| Module | Tests |
|--------|-------|
| `downloads.rs` | 7 — queue snapshot, cancel, remove, reorder, clear, cancel-all |
| `models.rs` | 6 — list empty, get not-found, add+list, missing file, remove not-found, tags |
| `settings.rs` | 4 — get defaults, directory info, memory threshold (Some/None) |
| `mcp.rs` | 4 — list empty, add+list, invalid type, remove |
| `setup.rs` | 1 — smoke test (get_status returns Ok) |
| `servers.rs` | 8 — 6 registry unit tests + list empty + stop non-existent |
