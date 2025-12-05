<!-- module-docs:start -->

# Web GUI Server Module

This module implements the HTTP server that powers the Web UI and provides the API for the frontend.

## Architecture

```text
┌─────────────┐      ┌────────────────┐      ┌──────────────────┐
│ Web Browser │ ───► │  Axum Router   │ ───► │   Route Handler  │
│ (React App) │      │ (routes.rs)    │      │ (handlers.rs)    │
└─────────────┘      └────────────────┘      └────────┬─────────┘
                                                      │
                                                      ▼
                                             ┌──────────────────┐
                                             │    GuiBackend    │
                                             │ (Shared Service) │
                                             └──────────────────┘
```

## Components

- **server.rs**: Configures and starts the Axum HTTP server.
- **routes.rs**: Defines the API route structure.
- **handlers.rs**: Contains the implementation for each API endpoint.
- **state.rs**: Manages the shared application state passed to handlers.

## Download Queue Endpoints

The following endpoints manage the download queue:

- `POST /api/models/download/queue` - Add a download to the queue
- `GET /api/models/download/queue` - Get current queue status
- `DELETE /api/models/download/queue/:id` - Remove a pending download from queue
- `DELETE /api/models/download/queue/failed` - Clear all failed downloads

## Server Logs Endpoints

The following endpoints provide access to llama-server stdout/stderr logs:

- `GET /api/servers/:port/logs` - Get buffered logs for a server
- `GET /api/servers/:port/logs/stream` - SSE stream of real-time log entries
- `DELETE /api/servers/:port/logs` - Clear buffered logs for a server

## Server Events Endpoint

- `GET /api/servers/events` - SSE stream of server lifecycle events. Provides web mode parity with Tauri's event system. Emits:
  - Initial `snapshot` of all running servers on connection
  - `running`, `stopping`, `stopped`, `crashed` events as they occur

## Message Endpoints

The following endpoints manage chat messages:

- `POST /api/messages` - Save a new message to the database
- `PUT /api/messages/:id` - Update a message's content
- `DELETE /api/messages/:id` - Delete a message and all subsequent messages (cascade deletion)

<!-- module-docs:end -->
