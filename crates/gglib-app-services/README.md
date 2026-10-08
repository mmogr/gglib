# gglib-app-services

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-complexity.json)

Backend facade for gglib's surfaces: `gglib-axum` builds the daemon's API on
it, and `gglib-cli` and the desktop app in `src-tauri` depend on it as well.

One implementation behind both GUIs, so the desktop app and the web UI cannot
drift apart: a capability added here appears on both at once, and both drive the
same shared runtime rather than competing for the GPU.

## Architecture

This crate is a **Shared Facade** — sitting between adapters and infrastructure, providing a unified orchestration layer.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 Adapter Layer                                       │
│      ┌────────────────┐    ┌────────────────┐    ┌────────────────┐                 │
│      │   gglib-axum   │    │   gglib-cli    │    │   src-tauri    │                 │
│      │   (HTTP API)   │    │     (CLI)      │    │ (desktop app)  │                 │
│      └────────┬───────┘    └────────┬───────┘    └────────┬───────┘                 │
│               │                     │                     │                         │
│               └─────────────────────┼─────────────────────┘                         │
│                                     ▼                                               │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                          ►►► gglib-app-services ◄◄◄                         │   │
│   │         Platform-agnostic GUI orchestration (ensures feature parity)        │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                     │                                               │
└─────────────────────────────────────┼───────────────────────────────────────────────┘
                                      ▼
              ┌───────────────────────────────────────────────────────┐
              │  gglib-core, gglib-runtime, gglib-proxy, gglib-agent, │
              │               gglib-download, gglib-mcp               │
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

**Module Descriptions:**
- **`downloads.rs`** — `DownloadOps` download queue and progress operations, and `search_hf_models`, the Hub search the browser runs through `DownloadOps` and `gglib model search` and `browse` call over the bootstrapped Hub client
- **`hf_quantizations.rs`** — A repository's quantizations as the `HuggingFace` browser shows them, each with the projector its download fetches
- **`hub_chats.rs`** — `HubChats`, the hub's chats as a paired device reads them, with each chat's live run
- **`error.rs`** — `GuiError` semantic error type for all app-service operations, and what a core error becomes in it (`From<CoreError>`): a refused input is a validation failure, a missing row is not found, a duplicate is a conflict, and only a failure of the store is internal
- **`mcp.rs`** — `McpOps` MCP server configuration and management
- **`models.rs`** — `ModelOps` model CRUD and listing operations
- **`models_projector.rs`** — The projector link on `ModelOps`: an update's `projector_path` applied through `ModelService::set_projector`, and the choices the inspector's picker offers
- **`models_upgrade.rs`** — `gglib model upgrade` on `ModelOps`: the commit check, and the download and row rewrite, whose progress is the download row handed to the caller's `RowCallback`
- **`proxy.rs`** — `ProxyOps` OpenAI-compatible proxy lifecycle management
- **`servers.rs`** — `ServerOps` llama.cpp server lifecycle management
- **`settings.rs`** — `SettingsOps` application settings persistence
- **`setup.rs`** — `SetupOps` first-run setup and dependency checking
- **`transcript.rs`** — What a turn writes to its conversation, for the daemon's agent runs and the CLI's chat alike: `save_user`, the user's message when the turn starts, and `save_reply`, the reply when it ends, whatever the end, rebuilt from the events the turn logged (`FrameTimes` holds when), each named for the model that made it (`MadeBy`); and `remember_thinking`, the Thinking choice a turn said, remembered on the conversation
- **`types.rs`** — Shared DTOs and type definitions for the service layer. Includes `UpdateModelRequest` (in `types_model_update.rs`), the request the inspector sends and `gglib model update` builds; its `apply_to` is what `ModelOps::update` writes it onto a model's row with. An omitted field is a no-op, and an empty `inference_defaults` returns the model to inheriting. It has triple-Option semantics for `server_defaults`: `Some(Some(cfg))` sets per-model server config, `Some(None)` clears it, and `None` (field omitted) is a no-op. `projector_path` has the same three states: a path links the model to that projector, `null` unlinks it, and an omitted key leaves the link alone.

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, etc.
2. **Pure Orchestration** — All deps injected via per-domain `*Deps` structs
3. **Trait-Based Injection** — Uses port traits, not concrete impls
4. **Semantic Errors** — Returns `GuiError`, adapters map to their error types
5. **Feature Parity** — Ensures desktop and web UIs have identical capabilities

## Usage

Adapters do not assemble the ops graph by hand — `build_service_graph`
does it once for both, enforcing the shared-`ProcessManager` ordering that
puts every llama-server on the machine behind one admission queue:

```rust,ignore
use gglib_app_services::{AppServices, ServiceGraphParams, build_service_graph};

let AppServices { models, servers, proxy, .. } =
    build_service_graph(ServiceGraphParams {
        core,
        repos,
        runner,
        // … adapter-supplied emitter, event sink and repositories …
        base_port: None, // defer to Settings.llama_base_port
        llama_server_path,
    })
    .await?;
```

Individual ops can still be constructed directly when only one is needed:

```rust
use gglib_app_services::{ModelOps, ModelDeps};
use std::sync::Arc;

# fn example(
#     core: Arc<gglib_core::services::AppCore>,
#     runtime: Arc<dyn gglib_core::ports::ModelRuntimePort>,
#     gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort>,
# ) {
let model_ops = ModelOps::new(ModelDeps {
    core: core.clone(),
    runtime: runtime.clone(),
    gguf_parser,
    // Library changes are broadcast so other clients see them. A one-shot
    // process with nobody listening passes `NoopEmitter::new()`.
    emitter: Arc::new(gglib_core::ports::NoopEmitter::new()),
});

// Use ops asynchronously in handlers
// let models = model_ops.list_with_query(ModelListQuery::default()).await?;
# }
```

## Testing

Each ops module has a `#[cfg(test)] mod tests`, inline or in a sibling `*_tests.rs` file.  Tests run against an
in-memory `SQLite` database provisioned by `gglib_db::setup_test_database()` and
`CoreFactory::build_repos()`, under an `AppCore::bare()`.  All external dependencies are replaced by
handwritten mock structs in `src/test_support.rs` (no external mocking framework).

| Module | Tests |
|--------|-------|
| `downloads.rs` | 7 — queue snapshot, cancel, remove, reorder, clear, cancel-all |
| `models.rs` | 6 — list empty, get not-found, add+list, missing file, remove not-found, tags |
| `settings.rs` | 16 — get defaults, profiles and nulls through the API, memory threshold (Some/None), and the models directory: saved and read back, its default, a refused path |
| `mcp.rs` | 11 — list empty, add+list, an SSE server refused on add, a stored SSE server listed as unsupported and refused a run and an edit, invalid type, remove, a taken name on add and on rename, servers already sharing a name, a failed test, a test of a missing server |
| `setup.rs` | 3 — smoke test (get_status returns Ok), and the memory floor and the directory and memory shapes the status shares with the settings routes |
| `servers.rs` | 9 — 6 registry unit tests + list empty + stop not-found + stop-all no-op |
