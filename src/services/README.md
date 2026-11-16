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
│  ┌─────────────┐  ┌─────────────┐                                                   │
│  │   server/   │  │  decoders/  │                                                   │
│  │  Safe calls │  │ Event decode│                                                   │
│  └─────────────┘  └─────────────┘                                                   │
│                                                                                     │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐                         │
│  │ serverRegistry │  │ proxyRegistry  │  │ remoteRegistry │                         │
│  │  Server state  │  │  Proxy state   │  │  Tunnel state  │                         │
│  └────────────────┘  └────────────────┘  └────────────────┘                         │
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
dialogs, menu sync, the frontend log bridge. That is OS
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
| `serverRegistry.ts` | External store for server lifecycle state. Uses `useSyncExternalStore` for reactive React integration. A snapshot (the list read) replaces the running set: a server it leaves out is stopped |
| `serverEvents.ts` | Subscribes to the daemon's SSE stream and ingests server lifecycle events into the registry; says whether the list's read has landed (`useServerListHeard`; until one has, every server event reads it again, as every opening of the stream does, and `keptCurrent` asks a failed read again while the stream is open) and why the last one failed (`useServerListError`), and reads it again on request (`refreshServers`, the chat route's Retry) |
| `serverEvents.normalize.ts` | Two named readers, one per producer: `normalizeServerEventFromAppEvent` for the camelCase `AppEvent` frames on `/api/events`, `normalizeServerSnapshotFromList` for the snake_case `GET /api/servers` list. Neither accepts the other's spelling |
| `proxyRegistry.ts` | External store for proxy state, the `serverRegistry.ts` analogue |
| `proxyEvents.ts` | Subscribes to proxy lifecycle events and ingests them into `proxyRegistry` |
| `hostEvents.ts` | Keeps `hostRegistry` current from `GET /api/version`'s `host` (`keptCurrent`: now and each time the event stream opens, a failed read asked again while it is open); kept by the main window and the tray panel; the last host stays while the service is gone, and an older service's answer without `host` leaves it as it was |
| `hostRegistry.ts` | External store for the machine the service runs on (`GET /api/version`'s `host`, kept by `hostEvents`); `thisMachine`/`useThisMachine`, the one way the app names that machine ("This Mac", "This PC", "This computer", "this computer" before the answer), and `fileName`, the one file-name helper, splitting a path on the host's separators or a Hugging Face repository's (`HF_REPO`) |
| `remoteRegistry.ts` | External store for the remote tunnel (ADR 0012): the daemon's status, both sides (an event moves the connection and its reach word at once, the read fills the rest), and the pairing code on offer with the timers that expire it, or lapse it once the daemon has let it go; and `stillPaired`, whether a far row, pick or chat held for the paired machine still holds once that status names another |
| `remoteRegistryState.ts` | What that store holds and what an empty tunnel looks like: the `RemoteState` shape and `IDLE_STATUS`, split out so the registry file stays under budget; and `pairedName`, the name the paired machine is shown by, never its fingerprint |
| `remoteOffer.ts` | The pairing code on offer, held by `remoteRegistry` so leaving the Devices place cannot lose it, and the pure rules for when a status read ends it (paired, tunnel down, or withdrawn) or marks it gone |
| `remoteEvents.ts` | Subscribes to `remote_*` events, ingests them into `remoteRegistry`, and re-reads the status after each |
| `modelRegistry.ts` | External store for this machine's model library, read once by every surface that shows it: the list as last read, whether a read has answered, and why the last one failed |
| `modelEvents.ts` | Fills `modelRegistry`: a read on init and on every opening of the event stream, one after each burst of `model_*` events (batched, so a retag of many is one read), and one after each edit made through its `addModel`/`removeModel`/`updateModel` |
| `downloadsRegistry.ts` | External store for the download queue, read by the library's Downloading group and the inspector's Download buttons: the daemon's snapshot, a newer `revision` replacing an older one whichever way it came, and the cancel that is out |
| `downloadsEvents.ts` | Fills `downloadsRegistry`: the queue read on init and on every opening of the event stream (the revision forgotten first, `onStreamOpen`, for a restarted daemon), and every `queue_snapshot` between (only a snapshot overtakes a read that is out; an ending carries none); settles a cancel on the download's ending or another's start; batches completions into one library read and one word to `onDownloadEndings`, failures told at once; queue, cancel and remove |
| `chatRegistry.ts` | External store for the chats: this machine's list and the joined machine's, newest first, read again when asked (the sidebar on mount and when a reply ends, the chat place after each change), a late read dropped, a failed read keeping the list and saying so (`failed`); `modelOf` (the model a conversation names, by its machine), `latestOn`/`latestOnFar`, `lastUsedModel`, and the delete and rename the sidebar and the title ▾ share |
| `dashboardRegistry.ts` | One `/v1/gglib/proxy/status/stream` per endpoint and credential, ref-counted: opened by the first surface that wants it, closed by the last, reopened when the key changes |
| `createEventStore.ts` | Shared store factory behind the registries: one value, replaced whole on each write, with a `useSyncExternalStore` hook over it |
| `bridgeEvents.ts` | `keptCurrent`, how every store that reads the daemon stays current: read now and again on every opening of the event stream (the first and each reconnection), so a first read the daemon refused is followed by one that lands; while the stream is open, a read that failed asked again after 1 s, doubling to 30 s, one retry pending per store at most, dropped when a read lands, an event or a write overtakes it, the stream closes or the store stops (the one timer a store's reads have: no store has one of its own); a read dropped when an event, a write, a stop or a later read overtook it, and a failure never newer than an earlier read. And the shared bridge behind the `*Events.ts` files, kept current the same way: subscribe before the hydrating fetch, and drop a fetch that an event (or only the events `overtakes` names) or a cleanup overtook |

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
| `menuEvents.ts` | Native menu bar event handling |
| `menuSync.ts` | Menu state synchronization |
| `openUrl.ts` | External URL opening |
| `serverLogs.ts` | Server log streaming |
| `index.ts` | The directory's public surface — what the rest of `src/` imports |
| `logging/` | Frontend log transports, bridged to Rust tracing via `log_from_frontend` |

## Transport Layer

The `transport/` directory provides a unified interface for backend communication:

- **Every mode**: HTTP fetch against the Axum API, plus SSE for events

Desktop and web share one transport. The desktop WebView resolves its base URL through the `get_embedded_api_info` IPC command and then consumes the same HTTP+SSE surface a browser tab does, so there is no second transport to keep in step — though `transport/api/client.ts` does still branch on platform to resolve that base URL and to choose its retry path. Beyond that, `invoke()` is confined to OS integration: six commands, allowlisted by name in `scripts/check-frontend-ipc.sh`.

<!-- module-docs:end -->
