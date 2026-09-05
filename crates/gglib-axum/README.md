# gglib-axum

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json)

HTTP API server for gglib — provides REST endpoints for the web UI and external integrations.

It also hosts the daemon: the single process that owns llama-server and holds
the exclusive lock every other surface connects through. When the CLI, the
desktop app, or the web UI needs the runtime, this is what they talk to.

## Architecture

This crate is in the **Adapter Layer** — it exposes gglib functionality via HTTP using the Axum framework.

```text
                              ┌──────────────────┐
                              │   gglib-axum     │
                              │   HTTP server    │
                              └────────┬─────────┘
                                       │
         ┌─────────────┬───────────────┬───────────────┬─────────────┐
         ▼             ▼               ▼               ▼             ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │gglib-download│ │gglib-runtime│ │  gglib-hf   │ │  gglib-mcp  │
│   SQLite    │ │  Downloads  │ │   Servers   │ │  HF client  │ │ MCP servers │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
         │             │               │               │             │
         └─────────────┴───────────────┴───────────────┴─────────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    gglib-core    │
                              │   (all ports)    │
                              └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-axum                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │   main.rs   │ ──► │ bootstrap.rs│ ──► │  routes.rs  │                            │
│  │  Entry pt   │     │  DI setup   │     │   Router    │                            │
│  │             │     │  & wiring   │     │  mounting   │                            │
│  └─────────────┘     └─────────────┘     └─────────────┘                            │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │    dto/     │     │  error.rs   │                                                │
│  │  Request &  │     │  HTTP error │                                                │
│  │  Response   │     │  handling   │                                                │
│  └─────────────┘     └─────────────┘                                                │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`access.rs`](src/access.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-access-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-access-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-access-coverage.json) |
| [`bootstrap.rs`](src/bootstrap.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-coverage.json) |
| [`chat_api.rs`](src/chat_api.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-coverage.json) |
| [`chat_api_tests.rs`](src/chat_api_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api_tests-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-coverage.json) |
| [`proxy_watch.rs`](src/proxy_watch.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy_watch-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy_watch-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy_watch-coverage.json) |
| [`routes.rs`](src/routes.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-coverage.json) |
| [`sse.rs`](src/sse.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-coverage.json) |
| [`state.rs`](src/state.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-coverage.json) |
| [`ui.rs`](src/ui.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui-coverage.json) |
| [`ui_tests.rs`](src/ui_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-ui_tests-coverage.json) |
| [`daemon/`](src/daemon/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-coverage.json) |
| [`dto/`](src/dto/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-coverage.json) |
| [`handlers/`](src/handlers/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`chat_api.rs`** — Chat completion API endpoints and streaming
- **`error.rs`** — HTTP error types and JSON error responses
- **`routes.rs`** — Route definitions and handler mounting
- **`sse.rs`** — Server-Sent Events utilities for streaming
- **`ui.rs`** — The dashboard, compiled into the binary, and its HTTP contract
- **`dto/`** — Request/response DTOs for API endpoints
- **`handlers/model/`** — Model CRUD, verification, downloads, HuggingFace discovery handlers
- **`handlers/config/`** — Settings and system setup handlers

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/models` | List all models |
| `POST` | `/api/models` | Add a new model |
| `DELETE` | `/api/models/:id` | Remove a model |
| `POST` | `/api/servers/start` | Start llama-server (id in the body) |
| `POST` | `/api/servers/stop` | Stop llama-server (id in the body) |
| `POST` | `/api/models/hf/search` | Search HuggingFace |
| `POST` | `/api/models/downloads/queue` | Queue a download |
| `GET` | `/api/models/downloads/queue` | Download queue snapshot |
| `GET` | `/api/config/settings` | Get application settings |
| `PUT` | `/api/config/settings` | Update application settings |
| `GET` | `/api/mcp/servers` | List MCP servers |
| `POST` | `/api/mcp/servers/:id/start` | Start MCP server |
| `POST` | `/api/models/:id/verify` | Verify model integrity (streams progress via SSE) |
| `GET` | `/api/models/:id/updates` | Check for HuggingFace updates |
| `POST` | `/api/models/:id/repair` | Re-download corrupt shards |

## Usage

This crate is a library — it has no binary target. The daemon that mounts it
is `gglib daemon run`, on a fixed loopback port:

```bash
# Start the daemon (this crate's router is what answers)
gglib daemon run

# Or have any command that needs it start one for you
gglib up
```

```rust,ignore
// Programmatic usage
use gglib_axum::start_server;
use gglib_axum::bootstrap::ServerConfig;

async fn run() -> anyhow::Result<()> {
    let config = ServerConfig::with_defaults()?;
    start_server(config).await
}
```

## Design Decisions

1. **Axum Framework** — Chosen for async-first design and tower middleware ecosystem
2. **Shared GuiBackend** — Same façade as Tauri for feature parity
3. **Thin Handlers** — No logic, just parse → delegate → serialize
4. **CORS Support** — Configurable CORS for web UI development
