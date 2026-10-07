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
│  │   daemon/   │ ──► │ bootstrap.rs│ ──► │  routes.rs  │                            │
│  │ run_daemon: │     │  DI setup   │     │   Router    │                            │
│  │ lock, serve │     │  & wiring   │     │  mounting   │                            │
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

**Module Descriptions:**
- **`daemon/`** — `run_daemon`, which `gglib daemon run` calls: the one process per machine that owns llama-server, and its singleton lock
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`config.rs`** — `ServerConfig`: what the server is given
- **`chat_api.rs`** — Conversations and their messages
- **`error.rs`** — HTTP error types and JSON error responses
- **`routes.rs`** — Route definitions and handler mounting
- **`sse.rs`** — Server-Sent Events utilities for streaming
- **`ui.rs`** — The dashboard, compiled into the binary, and its HTTP contract
- **`dto/`** — Request/response DTOs for API endpoints
- **`handlers/model/`** — Model CRUD, verification, downloads, `HuggingFace` discovery handlers
- **`handlers/config/`** — Settings and system setup handlers
- **`handlers/chat_title.rs`** — A chat's title, asked of the model the chat runs on

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/models` | List all models |
| `POST` | `/api/models` | Add a new model |
| `DELETE` | `/api/models/:id` | Remove a model |
| `PUT` | `/api/models/:id` | Update a model; `projectorPath` links it to a projector, `null` unlinks it |
| `GET` | `/api/models/:id/projectors` | The projector files the inspector's picker offers for a model |
| `POST` | `/api/servers/start` | Start llama-server (id in the body) |
| `POST` | `/api/servers/stop` | Stop llama-server (id in the body) |
| `POST` | `/api/models/hf/search` | Search `HuggingFace` |
| `POST` | `/api/models/downloads/queue` | Queue a download; answers `{ "id" }`, the download's ID |
| `GET` | `/api/models/downloads/queue` | Download queue snapshot |
| `POST` | `/api/models/downloads/:id/cancel` | Cancel a waiting or running download, every file of it |
| `DELETE` | `/api/models/downloads/:id` | The same; for a download that has ended, drop its finished entry |
| `POST` | `/api/models/downloads/finished/clear` | Clear the record of how earlier downloads ended |
| `GET` | `/api/config/settings` | Get application settings |
| `PUT` | `/api/config/settings` | Update application settings |
| `GET` | `/api/mcp/servers` | List MCP servers |
| `POST` | `/api/mcp/servers/:id/start` | Start MCP server |
| `POST` | `/api/models/:id/verify` | Verify model integrity (streams progress via SSE) |
| `GET` | `/api/models/:id/updates` | Check for `HuggingFace` updates |
| `POST` | `/api/models/:id/repair` | Re-download corrupt shards |
| `POST` | `/api/chat` | A chat's title: the text the model on a port answers a title request with (messages, a temperature and a token cap, and no other key) |
| `POST` | `/api/attachments` | Store an image, the raw body (a PNG or a JPEG of at most 8 MiB), and answer its id, type, size and estimated prompt tokens |
| `GET` | `/api/attachments/:id` | A stored image's bytes, as they were sent |
| `GET` | `/api/remote/models` | The paired machine's models, read through the tunnel, with what may be done to them there |
| `GET` | `/api/remote/models/:model` | One of the paired machine's models, read in full |
| `POST` | `/api/remote/models/:model/load` | Have one of the paired machine's models resident now |
| `POST` | `/api/remote/attachments` | Store an image on the paired machine, through the tunnel; nothing is kept here |
| `GET` | `/api/remote/attachments/:id` | One of the paired machine's stored images, sent `no-store` |

## Usage

This crate is a library — it has no binary target. The daemon that mounts it
is `gglib daemon run`, on a fixed loopback port:

```bash
# Start the daemon (this crate's router is what answers)
gglib daemon run

# Or have any command that needs it start one for you
gglib up
```

## Design Decisions

1. **Axum Framework** — Chosen for async-first design and tower middleware ecosystem
2. **Shared backend** — Handlers call `gglib-app-services`, the facade `gglib-cli` and `src-tauri` use too
3. **Thin Handlers** — No logic, just parse → delegate → serialize
4. **CORS Support** — Configurable CORS for web UI development
