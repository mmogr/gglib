<!-- module-docs:start -->

# Services Module

The services module contains the core business logic and shared services used by all frontends (CLI, Desktop GUI, Web UI).

## Architecture

```text
                     ┌────────────────┐
                     │   GuiBackend   │
                     │ (Coordinator)  │
                     └───────┬────────┘
                             │
          ┌──────────────────┼──────────────────┐
          │                  │                  │
          ▼                  ▼                  ▼
┌────────────────┐   ┌────────────────┐   ┌────────────────┐
│    Database    │   │ ProcessManager │   │     Proxy      │
│    (SQLite)    │   │ (llama-server) │   │ (OpenAI API)   │
└────────────────┘   └────────────────┘   └────────────────┘
```

## Components

- **gui_backend.rs**: The central coordinator service that exposes functionality to the GUI frontends. It manages state and orchestrates other services.
- **database.rs**: Handles all SQLite database interactions, including model metadata and chat history storage.
- **process_manager.rs**: Manages the lifecycle of external `llama-server` processes, including starting, stopping, and monitoring health.
- **chat_history.rs**: specialized service for managing conversation history.
- **proxy**: (Located in `src/proxy/`) The OpenAI-compatible proxy service.

## Key Workflows

### Server Startup Sequence
When a user requests to start a model server (via CLI or GUI), `GuiBackend` orchestrates the following:
1.  **Validation**: Checks if the model exists in the database and the file is accessible.
2.  **Port Allocation**: Finds a free port starting from the configured base port.
3.  **Process Launch**: Uses `ProcessManager` to spawn the `llama-server` binary with the correct arguments (context size, GPU layers, etc.).
4.  **Health Check**: Polls the server's `/health` endpoint until it responds or times out.
5.  **State Update**: Updates the internal state to track the running process ID and port.
6.  **Event Emission**: Emits a `server:running` event to notify the frontend.

### State Management
The `GuiBackend` maintains an in-memory state of all active processes. This state is not persisted to disk (except for chat history), meaning a restart of the application will lose track of externally running servers unless they are re-discovered (future feature).

## Frontend Services (TypeScript)

- **serverRegistry.ts**: Minimal external store for server lifecycle state. Uses `useSyncExternalStore` for reactive React integration. Backend events are the sole source of truth.
- **serverEvents.ts**: Platform adapter that auto-selects Tauri events (desktop) or SSE (web) for receiving lifecycle events.
- **serverEvents.tauri.ts**: Listens to Tauri `server:*` events and ingests them into the registry.
- **serverEvents.sse.ts**: Connects to `/api/servers/events` SSE endpoint for web mode parity.

### Server Event Types
- `server:snapshot` - Initial state of all running servers (emitted on app init)
- `server:running` - Server started and ready
- `server:stopping` - Server stop initiated
- `server:stopped` - Server stopped cleanly
- `server:crashed` - Server exited unexpectedly

<!-- module-docs:end -->
