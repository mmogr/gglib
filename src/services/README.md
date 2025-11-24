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

### State Management
The `GuiBackend` maintains an in-memory state of all active processes. This state is not persisted to disk (except for chat history), meaning a restart of the application will lose track of externally running servers unless they are re-discovered (future feature).

<!-- module-docs:end -->
