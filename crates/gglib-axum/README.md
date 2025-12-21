# gglib-axum

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json)

HTTP API server for gglib — provides REST endpoints for the web UI and external integrations.

## Architecture

This crate is in the **Adapter Layer** — it exposes gglib functionality via HTTP using the Axum framework.

```text
                              ┌──────────────────┐
                              │   gglib-axum     │
                              │   HTTP server    │
                              └────────┬─────────┘
                                       │
         ┌─────────────┬───────────────┼───────────────┬─────────────┐
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

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Architecture

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
| [`bootstrap.rs`](src/bootstrap) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-bootstrap-coverage.json) |
| [`chat_api.rs`](src/chat_api) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-chat_api-coverage.json) |
| [`embedded.rs`](src/embedded) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-embedded-coverage.json) |
| [`error.rs`](src/error) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-error-coverage.json) |
| [`routes.rs`](src/routes) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-routes-coverage.json) |
| [`sse.rs`](src/sse) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-sse-coverage.json) |
| [`state.rs`](src/state) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-state-coverage.json) |
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
- **`dto/`** — Request/response DTOs for API endpoints

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/models` | List all models |
| `POST` | `/api/models` | Add a new model |
| `DELETE` | `/api/models/:id` | Remove a model |
| `POST` | `/api/serve/:id` | Start llama-server |
| `DELETE` | `/api/serve/:id` | Stop llama-server |
| `POST` | `/api/hf/search` | Search HuggingFace |
| `POST` | `/api/download` | Queue a download |
| `GET` | `/api/download/:id` | Get download status |
| `GET` | `/api/mcp/servers` | List MCP servers |
| `POST` | `/api/mcp/servers/:id/start` | Start MCP server |

## Usage

```bash
# Start the API server
gglib-axum --port 9887

# Or run from the workspace
cargo run --package gglib-axum -- --port 9887
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
