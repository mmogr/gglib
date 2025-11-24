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

<!-- module-docs:end -->
