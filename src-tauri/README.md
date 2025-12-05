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

The Tauri application uses a hybrid architecture:

1. **Backend (Rust)**: The Tauri backend in `src-tauri/src/main.rs` uses the shared `GuiBackend` service from the main library. This is the same backend used by the Web UI.

2. **Embedded API Server**: When the desktop app starts, it spawns an embedded HTTP API server on `localhost` (similar to the standalone web server started by `gglib web`). This allows the React frontend to communicate with the backend using standard HTTP requests.

3. **Frontend (React)**: The React application in `src/` talks to the embedded API server via HTTP endpoints like:
   - `/api/models` - List and manage models
   - `/api/servers` - Control llama-server instances
   - `/api/chat` - Chat history and conversations
   - `/api/proxy` - Proxy management

4. **Tauri Commands**: Some operations can also use native Tauri IPC commands (the `invoke` API) as an alternative to HTTP for frontend-backend communication.

5. **Server Events**: The backend emits Tauri events (`server:running`, `server:stopping`, `server:stopped`, `server:crashed`, `server:snapshot`) that the frontend subscribes to for real-time server lifecycle updates. This is the authoritative source for server state - the frontend's `serverRegistry` ingests these events and components like `ConsoleInfoPanel` react to state changes.

This architecture means:
- The desktop GUI uses the exact same API routes as the Web UI
- Changes to the backend automatically benefit both interfaces
- Development and testing can happen in either mode
- Services like `TauriService` intelligently detect whether they're running in Tauri or Web mode

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
  - `tauri.conf.json`: Tauri configuration
