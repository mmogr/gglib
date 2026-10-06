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
│  │  API layer  │  │ HTTP + SSE  │  │ OS-specific │  │MCP tooling  │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                                  │
│  │   server/   │  │  registry   │  │  decoders/  │                                  │
│  │ Safe calls  │  │Server state │  │Event decode │                                  │
│  └─────────────┘  └─────────────┘  └─────────────┘                                  │
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
every mode, so it goes through the transport client against the same HTTP API
as everything else (`get` for the lines so far, `apiFetch` and the shared SSE
reader for the live stream), on a stream of its own rather than the pooled
SSE connection in `transport/events/`. It is listed below because it is here,
not because it belongs here.

## Directory Structure

| Directory | Description |
|-----------|-------------|
| [`clients/`](clients/) | The two clients that cannot go through `getTransport()`: streaming, or a non-backend origin |
| [`transport/`](transport/) | Transport layer — HTTP for requests, SSE for events — with type mappers |
| [`platform/`](platform/) | Platform-specific utilities (file dialogs, URL opening, menu sync) |
| [`tools/`](tools/) | MCP tool integration and builtin tool registry |
| [`server/`](server/) | Safe action wrappers for server operations |
| [`decoders/`](decoders/) | Runtime decoders that validate event payloads before ingestion |

## Key Files

| File | Description |
|------|-------------|
| `serverRegistry.ts` | External store for server lifecycle state. Uses `useSyncExternalStore` for reactive React integration. |
| `serverEvents.ts` | Subscribes to the daemon's SSE stream and ingests server lifecycle events into the registry |
| `serverEvents.normalize.ts` | Two named readers, one per producer: `normalizeServerEventFromAppEvent` for the camelCase `AppEvent` frames on `/api/events`, `normalizeServerSnapshotFromList` for the snake_case `GET /api/servers` list. Neither accepts the other's spelling |
| `proxyRegistry.ts` | External store for proxy state, the `serverRegistry.ts` analogue |
| `proxyEvents.ts` | Subscribes to proxy lifecycle events and ingests them into `proxyRegistry` |
| `remoteRegistry.ts` | External store for the remote tunnel (ADR 0012): the daemon's status, both sides; and `stillPaired`, whether a far row, pick or chat held for the paired machine still holds once that status names another |
| `remoteRegistryState.ts` | What that store holds and what an empty tunnel looks like: the `RemoteState` shape and `IDLE_STATUS`, split out so the registry file stays under budget; and `pairedName`, the name the paired machine is shown by, never its fingerprint |
| `remoteEvents.ts` | Subscribes to `remote_*` events, ingests them into `remoteRegistry`, and re-reads the status after each |
| `createEventStore.ts` | Shared store factory behind the three registries: one value, replaced whole on each write, with a `useSyncExternalStore` hook over it |
| `bridgeEvents.ts` | Shared bridge behind the three `*Events.ts` files: subscribe before the hydrating fetch, and drop a fetch that an event or a cleanup overtook |
| `agentOverrides.ts` | Per-session chat overrides, in two halves: `agentOverridesToWire()` builds the `config` object, `reasoningOverridesToWire()` builds the top-level reasoning fields the request declares separately |

## Clients

The `clients/` directory is deliberately small: a module belongs there only if
it needs streaming or a non-backend origin. Everything else goes through
`getTransport()`.

| Client | Description |
|--------|-------------|
| `benchmark.ts` | Benchmark and tune runs — REST endpoints plus an SSE progress stream |
| `proxyDashboard.ts` | Live proxy dashboard — fetch-based SSE against the running proxy's own port, carrying that proxy's credential |

## Server Event Types

Events are the source of truth for server state. They arrive from the daemon
over SSE (`/api/events`) — one path, desktop and web alike — and are normalized
into the registry's union by `serverEvents.normalize.ts`.

There are two ingestion paths, not one. The events are deltas, and none carries
the servers that were already running, so `initServerEvents` hydrates from
`GET /api/servers`. That list is a REST DTO, snake_case, and has its own
reader — writing `model_id` into an `AppEvent` fixture, or `modelId` into a
REST one, yields a silently empty registry rather than an error.
Note that camelCase on `AppEvent` is per-field `#[serde(rename)]`, not a
container rule: snake_case is serde's default here, so a newly added field is
snake_case unless someone remembers otherwise.

| `AppEvent` type | Description |
|-------|-------------|
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
