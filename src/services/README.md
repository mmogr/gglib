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
│  │  API layer  │  │ HTTP + SSE  │  │ OS-specific │  │MCP tooling  │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   server/   │  │    api/     │  │  registry   │  │  decoders/  │                 │
│  │ Safe calls  │  │   Routes    │  │Server state │  │Event decode │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                       │
                    ┌──────────────────┴──────────────────┐
                    ▼                                     ▼
            ┌──────────────┐                      ┌──────────────┐
            │ Axum (HTTP)  │                      │  SSE Events  │
            └──────────────┘                      └──────────────┘
```

`platform/` mostly reaches the OS through Tauri `invoke()`/`listen()` — file
dialogs, menu sync, llama installation, the frontend log bridge. That is OS
integration, not a transport.

`serverLogs.ts` is the exception and is misfiled: logs live on the daemon in
every mode, so it uses `fetch` and a raw `EventSource` against the same HTTP
API as everything else, bypassing the pooled SSE connection in
`transport/events/`. It is listed below because it is here, not because it
belongs here.

## Directory Structure

| Directory | Description |
|-----------|-------------|
| [`clients/`](clients/) | The two clients that cannot go through `Transport`: streaming, or a non-backend origin |
| [`transport/`](transport/) | Transport layer — HTTP for requests, SSE for events — with type mappers |
| [`platform/`](platform/) | Platform-specific utilities (file dialogs, URL opening, menu sync) |
| [`tools/`](tools/) | MCP tool integration and builtin tool registry |
| [`server/`](server/) | Safe action wrappers for server operations |
| [`api/`](api/) | Route definitions for API endpoints |
| [`decoders/`](decoders/) | Runtime decoders that validate event payloads before ingestion |

## Key Files

| File | Description |
|------|-------------|
| `serverRegistry.ts` | External store for server lifecycle state. Uses `useSyncExternalStore` for reactive React integration. |
| `serverEvents.ts` | Subscribes to the daemon's SSE stream and ingests server lifecycle events into the registry |
| `serverEvents.normalize.ts` | Normalises the wire's mixed-casing event payloads before ingestion |
| `proxyRegistry.ts` | External store for proxy state, the `serverRegistry.ts` analogue |
| `proxyEvents.ts` | Subscribes to proxy lifecycle events and ingests them into `proxyRegistry` |
| `createEventStore.ts` | Shared factory behind both registries — subscribe-before-fetch with an `eventVersion` guard |
| `agentOverrides.ts` | Per-session agent parameter overrides |

## Clients

The `clients/` directory is deliberately small: a module belongs there only if
it needs streaming or a non-backend origin. Everything else goes through the
`Transport` interface.

| Client | Description |
|--------|-------------|
| `benchmark.ts` | Benchmark and tune runs — REST endpoints plus an SSE progress stream |
| `proxyDashboard.ts` | Live proxy dashboard — fetch-based SSE against the running proxy's own port, carrying that proxy's credential |

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
| `index.ts` | The directory's public surface — what the rest of `src/` imports |
| `logging/` | Frontend log transports, bridged to Rust tracing via `log_from_frontend` |

## Transport Layer

The `transport/` directory provides a unified interface for backend communication:

- **Every mode**: HTTP fetch against the Axum API, plus SSE for events

Desktop and web share one transport. The desktop WebView resolves its base URL through the `get_embedded_api_info` IPC command and then consumes the same HTTP+SSE surface a browser tab does, so there is no second transport to keep in step — though `transport/api/client.ts` does still branch on platform to resolve that base URL and to choose its retry path. Beyond that, `invoke()` is confined to OS integration: seven commands, allowlisted by name in `scripts/check-frontend-ipc.sh`.

<!-- module-docs:end -->
