# GGLib Desktop GUI (Tauri)

This directory contains the Tauri-based desktop application for GGLib.

## Overview

The GGLib Desktop GUI is one of three complementary interfaces for managing GGUF models (along with the CLI and Web UI). All interfaces share the same backend architecture, ensuring consistent functionality and behavior.

For a complete overview of all interfaces and the shared architecture, see the main [README.md](../README.md#interfaces--modes).

## Architecture

The Desktop GUI is built using:
- **Tauri**: Rust-based application framework providing native OS integration
- **React**: Frontend UI library for the user interface
- **Vite**: Modern build tool and dev server
- **Assistant UI**: Chat interface components for conversational interactions

### How It Works

The Tauri application uses an **HTTP-first architecture** with minimal OS integration:

1. **Backend (Rust)**: The Tauri backend in `src-tauri/src/main.rs` uses the shared `GuiBackend` service from the main library. This is the same backend used by the Web UI.

2. **Embedded API Server**: When the desktop app starts, it spawns an embedded Axum HTTP server on `127.0.0.1` with an ephemeral port (OS-assigned). The server requires **Bearer token authentication** for all `/api/*` endpoints, providing security without exposing the API to other processes.

3. **Frontend (React)**: The React application in `src/` communicates **exclusively via HTTP** to the embedded API server:
   - `/api/models` - List and manage models
   - `/api/servers` - Control llama-server instances
   - `/api/chat` - Chat history and conversations
   - `/api/proxy` - Proxy management
   - `/api/downloads` - Download queue management
   - `/api/mcp` - MCP server configuration
   - `/api/events` - Server-Sent Events for real-time updates

4. **Tauri Commands (OS Integration Only)**: Tauri IPC commands are **limited to 6 OS integration functions**:
   - `get_embedded_api_info` - Discover API port and auth token
   - `open_url` - Open URLs in system browser
   - `set_selected_model`, `sync_menu_state` - Native menu synchronization
   - `check_llama_status`, `install_llama` - llama.cpp binary management

5. **Real-Time Events**: The `/api/events` endpoint streams Server-Sent Events with Bearer authentication:
   - `server:*` events - Server lifecycle updates
   - `download:*` events - Download progress
   - `log:*` events - Server console output

This architecture means:
- **Security**: All API access requires Bearer token, no unauthorized access to embedded server
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

The output binary will be located in `src-tauri/target/release/bundle/`.

## Key Features

**Multi-Interface Consistency:**
- All model operations (add, update, remove, serve) behave identically to the CLI and Web UI
- Process management is shared across all interfaces via the `ProcessManager` service
- Database changes are immediately visible in all interfaces
- Chat history is synchronized across Desktop GUI and Web UI

**Desktop-Specific Benefits:**
- Native OS integration (file dialogs, notifications, system tray)
- No need to manage separate server processes (embedded API server)
- Works offline once models are downloaded
- Better performance for local operations

For more details on the architecture and how all interfaces work together, see:
- [Interfaces & Modes](../README.md#interfaces--modes) in the main README
- [Architecture Overview](../README.md#architecture-overview) for backend details
- [LAN Server Mode](../README.md#running-gglib-as-a-lan-llm-server) for remote access options

## Project Structure

- `src/`: Frontend source code (React)
  - `components/`: UI components
  - `hooks/`: Custom React hooks
  - `services/`: API services (Tauri command wrappers)
- `src-tauri/`: Backend source code (Rust)
  - `src/main.rs`: Tauri application entry point
  - `src/app/`: Application state and event infrastructure
  - `src/menu/`: Native menu bar with stateful items
  - `src/commands/`: Tauri command handlers (organized by domain)
  - `tauri.conf.json`: Tauri configuration

## Backend Module Architecture

The Rust backend is organized into three main modules:

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           TAURI APPLICATION                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                          main.rs                                   │ │
│  │  • Tauri app setup (plugins, window, menu)                         │ │
│  │  • Embedded API server startup                                     │ │
│  │  • Command handler registration                                    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                    │                                     │
│          ┌─────────────────────────┼─────────────────────────┐          │
│          ▼                         ▼                         ▼          │
│  ┌──────────────┐         ┌──────────────┐         ┌──────────────┐    │
│  │    app/      │         │    menu/     │         │  commands/   │    │
│  │              │         │              │         │              │    │
│  │ • AppState   │◄───────►│ • AppMenu    │         │ • util       │    │
│  │ • Events     │         │ • MenuState  │         │ • llama      │    │
│  │ • emit_or_log│         │ • build      │         │   (OS-only)  │    │
│  │              │         │ • handlers   │         │              │    │
│  │              │         │ • state_sync │         │              │    │
│  │              │         │              │         │              │    │
│  └──────┬───────┘         └──────────────┘         └──────┬───────┘    │
│         │                                                  │            │
│         └──────────────────────┬───────────────────────────┘            │
│                                │                                         │
│                                ▼                                         │
│                    ┌───────────────────────┐                            │
│                    │      GuiBackend       │                            │
│                    │   (from gglib crate)  │                            │
│                    └───────────┬───────────┘                            │
│                                │                                         │
└────────────────────────────────┼─────────────────────────────────────────┘
                                 │
                                 ▼
                    ┌───────────────────────┐
                    │      gglib crate      │
                    │  • Database           │
                    │  • ProcessManager     │
                    │  • DownloadService    │
                    │  • HuggingFaceClient  │
                    │  • ProxyServer        │
                    └───────────────────────┘
```

### Module Responsibilities

| Module | Purpose | Key Components |
|--------|---------|----------------|
| **app/** | Central state & event infrastructure | `AppState` (managed state), `emit_or_log()` (event helper), event constants |
| **menu/** | Native menu bar with state sync | `AppMenu` (item refs), `MenuState`, menu builder, event handlers, state synchronization |
| **commands/** | 6 OS integration commands in 2 modules | `util.rs` (API discovery, shell, menu), `llama.rs` (binary management) |

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
│ GuiBackend      │◄─────────────────────────────│ AppState        │
│ .start_server() │                              │ .embedded_api   │
└────────┬────────┘                              └─────────────────┘
         │
         ▼
┌─────────────────┐
│ gglib-runtime   │
│ ProcessManager  │
└─────────────────┘
```

For detailed documentation on each module, see:
- [app/README.md](src/app/README.md) — State and event infrastructure
- [menu/README.md](src/menu/README.md) — Native menu implementation
- [commands/README.md](src/commands/README.md) — Tauri command reference
