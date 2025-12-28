<!-- module-docs:start -->

# Services Module

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

Events are the source of truth for server state. All events flow from the Rust backend:

| Event | Description |
|-------|-------------|
| `server:snapshot` | Initial state of all running servers (emitted on app init) |
| `server:started` | Server started and ready |
| `server:stopped` | Server stopped cleanly |
| `server:error` | Server encountered an error |
| `server:health_changed` | Server health status changed |

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

- **Tauri mode**: Uses `@tauri-apps/api/core.invoke()` for IPC
- **Web mode**: Uses standard HTTP fetch with the Axum API

This abstraction ensures identical behavior across platforms while allowing each to use its optimal transport mechanism.

<!-- module-docs:end -->
