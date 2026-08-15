<!-- module-docs:start -->

# Services Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-complexity.json)

The services module contains the TypeScript client layer for the gglib GUI frontends. These services provide a unified API for both Desktop (Tauri) and Web (Axum) platforms.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              React Components                                       │
└──────────────────────────────────────┬──────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                             services/ (This Module)                                 │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │  clients/   │  │ transport/  │  │  platform/  │  │   tools/    │                 │
│  │  API layer  │  │ HTTP/Tauri  │  │ OS-specific │  │MCP tooling  │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   server/   │  │    api/     │  │  registry   │  │   events    │                 │
│  │ Safe calls  │  │   Routes    │  │Server state │  │Event bridge │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                       │
               ┌───────────────────────┼───────────────────────┐
               ▼                       ▼                       ▼
       ┌──────────────┐        ┌──────────────┐        ┌──────────────┐
       │ Tauri (IPC)  │        │ Axum (HTTP)  │        │ SSE Events   │
       └──────────────┘        └──────────────┘        └──────────────┘
```

## Directory Structure

| Directory | Description |
|-----------|-------------|
| [`clients/`](clients/) | API client functions for each domain (models, servers, chat, downloads, etc.) |
| [`transport/`](transport/) | Platform-agnostic transport layer (Tauri IPC vs HTTP) with type mappers |
| [`platform/`](platform/) | Platform-specific utilities (file dialogs, URL opening, menu sync) |
| [`tools/`](tools/) | MCP tool integration and builtin tool registry |
| [`server/`](server/) | Safe action wrappers for server operations |
| [`api/`](api/) | Route definitions for API endpoints |

## Key Files

| File | Description |
|------|-------------|
| `serverRegistry.ts` | External store for server lifecycle state. Uses `useSyncExternalStore` for reactive React integration. |
| `serverEvents.ts` | Platform adapter that initializes Tauri events (desktop). Web uses unified SSE transport. |
| `serverEvents.tauri.ts` | Listens to Tauri `server:*` events and ingests them into the registry |

## Clients

The `clients/` directory contains domain-specific API functions:

| Client | Description |
|--------|-------------|
| `chat.ts` | Chat completion and conversation management |
| `downloads.ts` | Download queue operations and progress tracking |
| `events.ts` | Event subscription and handling |
| `huggingface.ts` | HuggingFace Hub search and model discovery |
| `mcp.ts` | MCP server configuration management |
| `models.ts` | Model CRUD operations |
| `servers.ts` | llama-server lifecycle management |
| `settings.ts` | Application settings |
| `system.ts` | System information and probes |
| `tags.ts` | Model tagging operations |

## Server Event Types

Events are the source of truth for server state. They arrive from the daemon
over SSE (`/api/events`) — one path, desktop and web alike — and are normalized
into the registry's union by `serverEvents.normalize.ts`:

| `AppEvent` type | Description |
|-------|-------------|
| `server_snapshot` | Initial state of all running servers (emitted at daemon startup) |
| `server_started` | Server started and ready |
| `server_stopped` | Server stopped cleanly |
| `server_error` | Server encountered an error |
| `server_health_changed` | Server health status changed |

There is no Tauri-event branch. There was one, listening for these same names
in `server:started` form on the Tauri bus, and nothing emitted them once the
GUI backend moved into the daemon — so the desktop registry was never
populated. `tests/ts/services/server/serverEvents.init.test.ts` pins the
single-path invariant against both platforms.

## Platform Utilities

The `platform/` directory provides OS-specific functionality:

| Utility | Description |
|---------|-------------|
| `detect.ts` | Platform detection (Tauri vs Web) |
| `fileDialogs.ts` | Native file picker integration |
| `llamaInstall.ts` | llama.cpp installation helpers |
| `menuEvents.ts` | Native menu bar event handling |
| `menuSync.ts` | Menu state synchronization |
| `openUrl.ts` | External URL opening |
| `serverLogs.ts` | Server log streaming |

## Transport Layer

The `transport/` directory provides a unified interface for backend communication:

- **Every mode**: HTTP fetch against the Axum API, plus SSE for events

Desktop and web share one transport. The desktop WebView resolves its base URL through the `get_embedded_api_info` IPC command and then consumes the same HTTP+SSE surface a browser tab does, so there is no per-platform branch to keep in step. `invoke()` survives only for the seven OS-integration commands allowlisted in `scripts/check-frontend-ipc.sh`.

<!-- module-docs:end -->
