<!-- module-docs:start -->

# App Module

The app module provides the central application state and event infrastructure for the Tauri desktop GUI.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         AppState (Managed)                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐ │
│  │    backend      │  │    api_port     │  │  selected_model_id  │ │
│  │  Arc<GuiBackend>│  │      u16        │  │ Arc<RwLock<Option>> │ │
│  └────────┬────────┘  └────────┬────────┘  └──────────┬──────────┘ │
│           │                    │                      │            │
│           │                    │                      │            │
│  ┌────────▼────────┐  ┌────────▼────────┐  ┌─────────▼──────────┐ │
│  │     menu        │  │ Embedded HTTP   │  │   Menu State Sync  │ │
│  │ Arc<RwLock<     │  │ Server (axum)   │  │   (enable/disable  │ │
│  │   AppMenu>>     │  │ localhost:port  │  │    items based on  │ │
│  └─────────────────┘  └─────────────────┘  │    selection)      │ │
│                                            └────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌──────────────┐    ┌──────────────────┐    ┌──────────────┐
│  commands/*  │    │     menu/*       │    │  React UI    │
│  (read state)│    │ (sync state)     │    │  (via HTTP)  │
└──────────────┘    └──────────────────┘    └──────────────┘
```

## Components

- **mod.rs**: Re-exports `state` and `events` submodules.
- **state.rs**: Defines `AppState` — the central state container managed by Tauri. Holds the shared `GuiBackend`, embedded API port, native menu references, and currently selected model ID.
- **events.rs**: Provides `emit_or_log()` helper for fire-and-forget event emission, plus constants for all Tauri event names (download progress, server lifecycle, menu actions).

## Event System

The Tauri application uses events for real-time communication between the Rust backend and React frontend.

```text
Backend Operation (server start, download progress, etc.)
          │
          ▼
    ┌─────────────────────┐
    │  emit_or_log()      │◄── Logs errors instead of panicking
    │  (events.rs)        │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  Tauri Event Bus    │
    │                     │
    │  Events:            │
    │  • download-progress│
    │  • server:running   │
    │  • server:stopped   │
    │  • server:snapshot  │
    │  • menu:*           │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  React Listeners    │
    │  (useEffect hooks)  │
    └─────────────────────┘
```

### Event Constants

| Category | Events |
|----------|--------|
| Downloads | `DOWNLOAD_PROGRESS` |
| Server Lifecycle | `SERVER_RUNNING`, `SERVER_STOPPING`, `SERVER_STOPPED`, `SERVER_CRASHED`, `SERVER_SNAPSHOT` |
| Menu Actions | `MENU_ADD_MODEL`, `MENU_REMOVE_MODEL`, `MENU_BROWSE_HUGGINGFACE`, `MENU_START_SERVER`, `MENU_STOP_SERVER`, `MENU_OPEN_CHAT`, `MENU_INSTALL_LLAMA` |

<!-- module-docs:end -->
