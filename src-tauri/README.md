# GGLib Desktop GUI (Tauri)

This directory contains the Tauri-based desktop application for GGLib.

## Overview

The GGLib Desktop GUI is a client of the daemon that owns the runtime — a place to watch the proxy and manage the model library, not the thing serving requests. It launches or connects to a daemon and can run it as a background service from the tray, so the endpoint keeps serving after the window closes. The CLI and Web UI are clients of the same daemon, sharing one backend and one database.

For a complete overview of all interfaces and the shared architecture, see the main [README.md](../README.md#interfaces).

## Architecture

The Desktop GUI is built using:
- **Tauri**: Rust-based application framework providing native OS integration
- **React**: Frontend UI library for the user interface
- **Vite**: Modern build tool and dev server
- **Assistant UI**: Chat interface components for conversational interactions

### How It Works

The Tauri application uses an **HTTP-first architecture** with minimal OS integration:

1. **Backend (Rust)**: The Tauri backend in `src-tauri/src/main.rs` owns no runtime of its own. `src/daemon/` connects to the gglib daemon, launching `gglib daemon run` detached when nothing answers, and hosting the daemon composition in-process only as a bundle-only fallback. A daemon that will not start is a state, not a crash: the app comes up disconnected, and the tray says so.

2. **The daemon's API**: Everything backend-shaped goes to `127.0.0.1:{DAEMON_PORT}` — a fixed loopback port, deliberately a constant rather than a setting, so every client can find the one daemon without configuration. Loopback traffic is unauthenticated; the stored API key is required only for a LAN-shared daemon (`gglib daemon run --share-lan`).

3. **Frontend (React)**: The React application in `src/` communicates **exclusively via HTTP** to the daemon:
   - `/api/models` - List and manage models
   - `/api/servers` - Control llama-server instances
   - `/api/chat` - Chat history and conversations
   - `/api/proxy` - Proxy management
   - `/api/downloads` - Download queue management
   - `/api/mcp` - MCP server configuration
   - `/api/events` - Server-Sent Events for real-time updates

4. **System Tray**: The tray icon, its menu and the popover panel live in `src/tray/`. The panel is a second window loading its own Vite entry (`tray.html`), and uses **no Tauri IPC at all** — it reaches the daemon through the same HTTP transport as every other surface. Window-level actions (open, preferences, quit, and starting or stopping the service) are on the native tray menu, handled in Rust, which is what keeps the command list below unchanged. The icon tracks whether gglib is consuming anything on this machine — a listening proxy *or* a resident model — from a snapshot polled by `src/daemon/watch.rs`. See [src/tray/README.md](src/tray/README.md).

5. **Tauri Commands (OS Integration Only)**: Tauri IPC commands are **limited to OS integration**:
   - `get_embedded_api_info` - Discover the daemon's port (the name predates the daemon)
   - `open_url` - Open URLs in system browser
   - `set_selected_model`, `sync_menu_state` - Native menu synchronization
   - `check_llama_status`, `install_llama` - llama.cpp binary management
   - `log_from_frontend` - Forward frontend logs to the Rust logger

6. **Real-Time Events**: The daemon's `/api/events` endpoint streams Server-Sent Events to each webview:
   - `server:*` events - Server lifecycle updates
   - `download:*` events - Download progress
   - `proxy:*` events - Proxy lifecycle

   The Rust side deliberately does not subscribe: it polls instead, because the lifecycle events are deltas that a lagging subscriber drops silently, and a tray rebuilt from deltas drifts. See `src/daemon/watch.rs`.

This architecture means:
- **Consistency**: Desktop GUI uses identical HTTP API as standalone Web UI
- **Simplicity**: Business logic lives in one place (Axum handlers), not duplicated in IPC commands
- **Testability**: HTTP API can be tested with standard tools (curl, Postman, etc.)
- **Portability**: Web mode works identically to Tauri mode (frontend auto-detects environment)

## Development Setup

### Prerequisites
- Node.js (v18+)
- Rust (v1.70+)
- System dependencies for Tauri (see [Tauri docs](https://tauri.app/v1/guides/getting-started/prerequisites))

### Running in Development Mode

1. Install dependencies:
   ```bash
   npm install
   ```

2. Run the development server:
   ```bash
   npm run tauri:dev
   ```
   This will start the Vite dev server and launch the Tauri application window.

### Building for Production

To build the application for your platform:

```bash
npm run tauri:build
```

The output binary will be located in `target/release/bundle/`.

## Key Features

**Multi-Interface Consistency:**
- All model operations (add, update, remove, serve) behave identically to the CLI and Web UI
- Process management is shared across all interfaces via the `ProcessManager` service
- Database changes are immediately visible in all interfaces
- Chat history is synchronized across Desktop GUI and Web UI

**Desktop-Specific Benefits:**
- Native OS integration (file dialogs, notifications, system tray)
- The daemon is launched for you; no separate server process to manage
- Works offline once models are downloaded
- Better performance for local operations

**Stability & Resource Management:**
- **One owner**: llama-server belongs to the daemon, so the app has almost nothing to tear down
- **Honest quit**: the daemon this app launched or hosts is stopped with it; one it merely connected to is left serving
- **Ordered teardown**: proxy drained, children stopped gracefully, downloads cancelled, pidfiles audited — all in the daemon, under its own watchdog
- **PID file audit**: Final safety net catches any orphaned llama-server processes
- **No resource leaks**: Proper cleanup prevents thread exhaustion and zombie processes

For more details on the architecture and how all interfaces work together, see:
- [Interfaces](../README.md#interfaces) in the main README
- [Architecture Overview](../README.md#architecture) for backend details

## Project Structure

- `src/`: Frontend source code (React)
  - `components/`: UI components
  - `hooks/`: Custom React hooks
  - `services/`: API services (Tauri command wrappers)
- `src-tauri/`: Backend source code (Rust)
  - `src/main.rs`: Tauri application entry point
  - `src/app/`: Application state and event infrastructure
  - `src/lifecycle.rs`: Hardened shutdown orchestration with watchdog
  - `src/menu/`: Native menu bar with stateful items (macOS only)
  - `src/tray/`: System tray icon, menu and popover panel (all platforms)
  - `src/proxy_actions.rs`: Proxy start/stop shared by the tray and autostart
  - `src/autostart.rs`: Always-on proxy startup and OS login-item registration
  - `src/dock.rs`: macOS Dock icon visibility (activation policy)
  - `src/commands/`: Tauri command handlers (organized by domain)
  - `tauri.conf.json`: Tauri configuration

## Backend Module Architecture

The Rust backend is organized into three main modules:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           TAURI APPLICATION                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                          main.rs                                   │ │
│  │  • Tauri app setup (plugins, window, menu)                         │ │
│  │  • Daemon connect-or-launch, then spawn the watcher                │ │
│  │  • Command handler registration                                    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                    │
│          ┌─────────────────────────┼─────────────────────────┐          │
│          ▼                ▼             ▼                    ▼          │
│  ┌──────────────┐  ┌────────────┐  ┌────────────┐   ┌──────────────┐    │
│  │    app/      │  │   menu/    │  │   tray/    │   │  commands/   │    │
│  │              │  │  (macOS)   │  │  (all OS)  │   │              │    │
│  │ • AppState   │◄─┤ • AppMenu  │  │ • build    │   │ • util       │    │
│  │ • Events     │  │ • MenuState│  │ • icon     │   │ • llama      │    │
│  │ • emit_or_log│  │ • build    │  │ • handlers │   │ • app_logs   │    │
│  │              │  │ • handlers │  │ • confirm  │   │   (OS-only)  │    │
│  │              │  │ • state_sync──►│ • window  │   │              │    │
│  │              │  │            │  │            │   │              │    │
│  └──────┬───────┘  └────────────┘  └─────┬──────┘   └──────┬───────┘    │
│         │                             │                   │             │
│         └─────────────────────┬─────────┴───────────────────┘           │
│                                │                                        │
│                                ▼                                        │
│                    ┌───────────────────────┐                            │
│                    │       daemon/         │                            │
│                    │  • connect_or_launch  │                            │
│                    │  • DaemonSnapshot     │                            │
│                    │  • watch (2s poll)    │                            │
│                    └───────────┬───────────┘                            │
│                                │                                        │
└────────────────────────────────┼─────────────────────────────────────────┘
                                 │ HTTP, 127.0.0.1:9887
                                 ▼
                    ┌───────────────────────┐
                    │    gglib daemon       │
                    │  • Database           │
                    │  • ProcessManager     │
                    │  • DownloadService    │
                    │  • HuggingFaceClient  │
                    │  • ProxyServer        │
                    └───────────────────────┘
```

The app hosts none of that. It finds a daemon, watches it, and paints two OS
surfaces from what it sees — see [`daemon/README.md`](src/daemon/README.md) for
the three ways it comes by one and what quitting is allowed to take with it.

### Module Responsibilities

| Module | Purpose | Key Components |
|--------|---------|----------------|
| **app/** | Central state & event infrastructure | `AppState` (daemon, menu, tray, selected model, snapshot, refresh), `emit_or_log()` (event helper), event constants |
| **daemon/** | The connection, and the picture of it | `connect_or_launch()` / `Ownership` (adopted, launched, hosted, unresolved), `DaemonSnapshot` (pure derivation), `watch` (the only writer of that snapshot) |
| **lifecycle.rs** | Application startup & hardened shutdown | `request_shutdown()` (single guarded entry point), `is_shutting_down()` / `should_prevent_exit()` (exit re-entrancy), `perform_shutdown()` (daemon teardown, bounded, and only when the daemon is ours) |
| **menu/** | macOS menu bar with state sync | `AppMenu` (item refs), `MenuState`, menu builder, event handlers, `sync_all_state` (drives both the menu and the tray) |
| **tray/** | System tray icon, menu and panel | `build` (icon/menu), `icon` (pure state → icon/tooltip), `handlers` (thin dispatch), `confirm` (what a teardown would take away), `window` (panel show/hide/position) |
| **proxy_actions.rs** | Proxy start/stop outside a request | Used by the tray and autostart; calls the daemon's `/api/proxy/*` directly and asks for a fresh poll — it deliberately does **not** publish what it expects to be true |
| **autostart.rs** | Launch visibility & login item | `start_at_login` login item, `should_start_hidden()` (pure launch decision, fails visible). Proxy autostart is the daemon's job |
| **dock.rs** | macOS Dock icon visibility | `hide()` / `show()` via activation policy; no-ops off macOS so callers need no `cfg` |
| **commands/** | 6 OS integration commands in 3 modules | `util.rs` (API discovery, shell, menu), `llama.rs` (binary management), `app_logs.rs` (frontend log ingestion) |

### Communication Flow

```text
┌─────────────────┐                              ┌─────────────────┐
│   React UI      │                              │  Native Menu    │
│   (Frontend)    │                              │  (macOS/Win/Lin)│
└────────┬────────┘                              └────────┬────────┘
         │                                                │
         │  HTTP: POST /api/servers/start               │  Click "Start Server"
         │  Tauri: invoke("open_url")                   │
         │                                                │
         ▼                                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                  HTTP API (primary) / Tauri IPC (OS)                │
└────────┬───────────────────────────────────────────────────┬────────┘
         │                                                   │
         ▼                                                   ▼
┌─────────────────┐                              ┌─────────────────┐
│ gglib-axum      │                              │ commands/       │
│ handlers/       │                              │ util.rs         │
│ servers.rs      │                              │ (API discovery, │
└────────┬────────┘                              │  shell, menu)   │
         │                                       └────────┬────────┘
         │                                                │
         ▼                                                ▼
┌─────────────────┐                              ┌─────────────────┐
│ gglib-runtime   │                              │ AppState        │
│ ProcessManager  │                              │ .snapshot       │
└────────┬────────┘                              └────────▲────────┘
         │                                                │
         └──────────────► /api/proxy/status ──────────────┘
                          /api/servers          daemon/watch, every 2s
```

Both columns end at the same daemon. The native surfaces never learn what
happened by being told: `daemon/watch` polls, and every action asks it for an
immediate poll rather than publishing a guess.

## Internal Structure

For detailed documentation on each module, see:
- [app/README.md](src/app/README.md) — State and event infrastructure
- [daemon/README.md](src/daemon/README.md) — Finding a daemon, watching it, owning it
- [LIFECYCLE.md](LIFECYCLE.md) — Application startup and hardened shutdown architecture
- [menu/README.md](src/menu/README.md) — Native menu implementation (macOS)
- [tray/README.md](src/tray/README.md) — System tray, popover panel, and platform differences
- [commands/README.md](src/commands/README.md) — Tauri command reference
