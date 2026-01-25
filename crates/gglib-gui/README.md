# gglib-gui

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-complexity.json)

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
│   │                         ►►► gglib-gui ◄◄◄                                   │   │
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

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-gui                                           │
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
│                         ┌─────────────┐                                             │
│                         │     mcp     │                                             │
│                         │   McpOps    │                                             │
│                         └─────────────┘                                             │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`backend.rs`](src/backend.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-backend-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-backend-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-backend-coverage.json) |
| [`deps.rs`](src/deps.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-deps-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-deps-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-deps-coverage.json) |
| [`downloads.rs`](src/downloads.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-downloads-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-downloads-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-downloads-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-error-coverage.json) |
| [`mcp.rs`](src/mcp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-mcp-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-models-coverage.json) |
| [`proxy.rs`](src/proxy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-proxy-coverage.json) |
| [`servers.rs`](src/servers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-servers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-servers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-servers-coverage.json) |
| [`settings.rs`](src/settings.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-settings-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-settings-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-settings-coverage.json) |
| [`types.rs`](src/types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-types-coverage.json) |
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
use gglib_gui::{GuiBackend, GuiDeps, GuiError};
use gglib_gui::{ModelOps, DownloadOps, ServerOps};

// Construct with dependency injection
let deps = GuiDeps::new(
    model_repo,
    hf_client,
    download_manager,
    process_manager,
    settings_store,
    event_emitter,
);

let backend = GuiBackend::new(deps);

// Use operation modules
let models = backend.models().list_all().await?;
let queue = backend.downloads().get_queue_snapshot().await?;
```
