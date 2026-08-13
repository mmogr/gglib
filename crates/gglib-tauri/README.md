# gglib-tauri

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json)
![Coverage N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-complexity.json)

Desktop GUI backend for gglib — Tauri application with React frontend.

The desktop app is a client of the daemon, not an owner of the runtime: it
launches or connects to one, then watches. That is what lets the proxy keep
serving after the window closes, and why the tray can run it as a background
service.

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
         ┌─────────────┬───────────────┬───────────────────┬───────────┐
         ▼             ▼               ▼                   ▼           ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │gglib-download│ │gglib-runtime│ │  gglib-hf   │ │  gglib-mcp  │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

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
| [`events.rs`](src/events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-events-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`events.rs`** — The names of every event the app emits to its webviews, and `emit_or_log`

This crate is deliberately two files. It used to carry the whole GUI backend —
`bootstrap.rs`, `error.rs`, `event_emitter.rs`, `server_events.rs`,
`gui_backend/` — back when the desktop app hosted gglib's services itself. The
daemon consolidation moved all of that behind HTTP, and a `TauriEmitter`
implementation of `AppEventEmitter` no longer exists: the daemon's own
`SseBroadcaster` is the only emitter, and every webview subscribes to
`/api/events` like any other client.

## Features

- **Event names in one place** — so a Rust emitter and a TypeScript listener
  cannot drift apart silently
- **`emit_or_log`** — emit to every webview, log rather than propagate on
  failure; a webview that has gone away is not an error worth unwinding for

## IPC Commands

The desktop app is **HTTP-first** — models, chat, downloads and proxy
management all go through the daemon's Axum API. Tauri IPC is limited to things
HTTP cannot do, and those commands live in
[`src-tauri/src/commands/`](../../src-tauri/src/commands/), not here:

| Command | Module | Description |
|---------|--------|-------------|
| `get_embedded_api_info` | util | Discover the daemon's port |
| `open_url` | util | Open URL in system browser |
| `set_selected_model` | util | Sync native menu selection |
| `sync_menu_state` | util | Update native menu item states |
| `check_llama_status` | llama | Check llama.cpp installation |
| `install_llama` | llama | Install/build llama.cpp |
| `build_llama_from_source` | llama | Build llama.cpp from source |
| `log_from_frontend` | app_logs | Forward frontend logs to Rust logger |

`scripts/check-frontend-ipc.sh` holds the allowlist this table describes; the
two are checked against each other in CI.

See [src-tauri/README.md](../../src-tauri/README.md) for the full architecture explanation.

## Events

Real-time events are delivered via SSE (`/api/events`) with Bearer auth, not Tauri emit:

| Event | Description |
|-------|-------------|
| `server:*` | Server lifecycle (start, ready, stop, error) |
| `download:*` | Download progress and completion |
| `log:*` | Server console output |

## Usage

```bash
# Development (with hot reload)
npm run tauri dev

# Build for production
npm run tauri build
```

## Design Decisions

1. **One event bus, not two** — the daemon's SSE stream carries every domain
   event to every client. Tauri emit is reserved for things that originate in
   the app itself (menu clicks, llama build progress), which have no HTTP source
2. **Names, not payloads** — this crate owns the event *names* so a Rust
   emitter and a TypeScript listener cannot drift; the payloads are the domain
   types the daemon already serialises
3. **Emit is best-effort** — `emit_or_log` logs rather than propagates. A
   webview that has gone away is not an error worth unwinding a menu handler for
