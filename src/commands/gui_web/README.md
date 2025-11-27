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

<!-- module-docs:end -->
