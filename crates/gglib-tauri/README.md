# gglib-tauri

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json)
![Coverage N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-complexity.json)

Desktop GUI backend for gglib — Tauri application with React frontend.

> **Note:** Coverage metrics are not tracked for this crate due to GTK system library dependencies required by Tauri.

## Architecture

This crate is in the **Adapter Layer** — it provides the Tauri backend that bridges the React UI to gglib services.

```text
                        ┌────────────────────────────────────┐
                        │           gglib-tauri              │
                        │         Desktop GUI app            │
                        └───────────────┬────────────────────┘
                                        │
                    ┌───────────────────┼───────────────────┐
                    ▼                   │                   ▼
          ┌──────────────────┐          │         ┌──────────────────┐
          │   React UI (TS)  │◄─────────┴────────►│  Tauri Backend   │
          │   src/components │   IPC Commands     │  (this crate)    │
          └──────────────────┘                    └────────┬─────────┘
                                                           │
         ┌─────────────┬───────────────┬───────────────────┼───────────┐
         ▼             ▼               ▼                   ▼           ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │gglib-download│ │gglib-runtime│ │  gglib-hf   │ │  gglib-mcp  │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                               gglib-tauri                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌──────────────────────────────────────┐   │
│  │   lib.rs    │ ──► │bootstrap.rs │ ──► │           gui_backend/               │   │
│  │  Tauri app  │     │  DI setup   │     │  ┌────────────┐  ┌────────────────┐  │   │
│  │  commands   │     │  & wiring   │     │  │  commands  │  │  event_bridge  │  │   │
│  └─────────────┘     └─────────────┘     │  │  (IPC)     │  │  (Tauri emit)  │  │   │
│                                          │  └────────────┘  └────────────────┘  │   │
│  ┌─────────────┐     ┌─────────────┐     │  ┌────────────┐  ┌────────────────┐  │   │
│  │  error.rs   │     │event_emitter│     │  │   state    │  │     ...        │  │   │
│  │  IPC errors │     │ TauriEmitter│     │  │  (shared)  │  │                │  │   │
│  └─────────────┘     └─────────────┘     │  └────────────┘  └────────────────┘  │   │
│                                          └──────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`bootstrap.rs`](src/bootstrap) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-bootstrap-coverage.json) |
| [`error.rs`](src/error) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-error-coverage.json) |
| [`event_emitter.rs`](src/event_emitter) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-event_emitter-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-event_emitter-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-event_emitter-coverage.json) |
| [`events.rs`](src/events) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-coverage.json) |
| [`server_events.rs`](src/server_events) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-server_events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-server_events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-server_events-coverage.json) |
| [`gui_backend/`](src/gui_backend/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-gui_backend-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-gui_backend-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-gui_backend-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`error.rs`** — IPC-compatible error types
- **`event_emitter.rs`** — `TauriEmitter` implementation of `AppEventEmitter`
- **`events.rs`** — Event type definitions and serialization
- **`server_events.rs`** — Server-specific event handling
- **`gui_backend/`** — IPC command handlers and shared state

## Features

- **IPC Commands** — Tauri commands expose gglib services to the React UI
- **Event Bridge** — `TauriEmitter` sends real-time events to the frontend
- **Shared State** — Managed state accessible across all commands
- **Native Dialogs** — File picker, notifications via Tauri APIs

## IPC Commands

| Command | Description |
|---------|-------------|
| `list_models` | Get all models |
| `add_model` | Add a GGUF file |
| `remove_model` | Delete a model |
| `serve_model` | Start llama-server |
| `stop_server` | Stop a running server |
| `search_hf` | Search HuggingFace |
| `download_model` | Queue a download |
| `get_download_status` | Poll download progress |
| `list_mcp_servers` | Get MCP server configs |
| `start_mcp_server` | Start an MCP server |

## Events

Events are emitted to the frontend via Tauri's event system:

| Event | Payload |
|-------|---------|
| `download:progress` | `{ id, progress, speed, eta }` |
| `download:complete` | `{ id, path }` |
| `server:status` | `{ id, status, port }` |
| `mcp:tool_result` | `{ server_id, tool, result }` |

## Usage

```bash
# Development (with hot reload)
npm run tauri dev

# Build for production
npm run tauri build
```

## Design Decisions

1. **TauriEmitter** — Implements `AppEventEmitter` to bridge Rust events to JS
2. **State Injection** — Services stored in Tauri's managed state
3. **Command Pattern** — Each IPC command maps to a service method
4. **Error Serialization** — All errors converted to JSON for frontend
