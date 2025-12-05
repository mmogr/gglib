<!-- module-docs:start -->

# Menu Module

The menu module implements the native cross-platform menu bar with stateful items that reflect application state.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           Native Menu Bar                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │   File   │ │   Edit   │ │  Model   │ │  Proxy   │ │   Help   │      │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘      │
│       │            │            │            │            │             │
│       ▼            ▼            ▼            ▼            ▼             │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐       │
│  │ Add     │  │ Cut     │  │ Start ▸ │  │ ☑ Enable│  │ About   │       │
│  │ Remove  │  │ Copy    │  │ Stop    │  │ Copy URL│  │ Docs    │       │
│  │ Browse  │  │ Paste   │  │ Restart │  │ Open    │  │ GitHub  │       │
│  │ Quit    │  │ Select  │  │ ─────── │  └─────────┘  └─────────┘       │
│  └─────────┘  └─────────┘  │ Install │                                 │
│                            │ llama   │                                 │
│                            └─────────┘                                 │
└─────────────────────────────────────────────────────────────────────────┘
```

## Components

- **mod.rs**: Exports submodules and defines `AppMenu` (holds references to stateful menu items) and `MenuState` (current state for synchronization).
- **ids.rs**: String constants for menu item IDs (e.g., `START_SERVER`, `PROXY_TOGGLE`).
- **build.rs**: `build_app_menu()` — constructs the complete menu hierarchy, returns `(Menu, AppMenu)`.
- **handlers.rs**: `handle_menu_event()` — dispatches menu clicks to frontend via Tauri events.
- **state_sync.rs**: `sync_menu_state_internal()` — updates menu enabled/checked states based on current app state.

## Menu State Synchronization

Menu items are enabled/disabled based on application state. This keeps the native menu in sync with what actions are actually available.

```text
┌──────────────────────────────────────────────────────────────────────┐
│                        State Change Trigger                          │
│  (model selected, server started, proxy toggled, llama installed)    │
└───────────────────────────────┬──────────────────────────────────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │  sync_menu_state()    │
                    │  (commands/util.rs)   │
                    └───────────┬───────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │ sync_menu_state_      │
                    │ internal()            │
                    │ (menu/state_sync.rs)  │
                    └───────────┬───────────┘
                                │
            ┌───────────────────┼───────────────────┐
            ▼                   ▼                   ▼
    ┌───────────────┐   ┌───────────────┐   ┌───────────────┐
    │ check_llama   │   │ get_proxy     │   │ list_servers  │
    │ _installed()  │   │ _status()     │   │ ()            │
    └───────┬───────┘   └───────┬───────┘   └───────┬───────┘
            │                   │                   │
            └───────────────────┼───────────────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │  MenuState {          │
                    │    llama_installed,   │
                    │    proxy_running,     │
                    │    proxy_url,         │
                    │    model_selected,    │
                    │    server_running,    │
                    │  }                    │
                    └───────────┬───────────┘
                                │
                                ▼
                    ┌───────────────────────┐
                    │  AppMenu::sync_state()│
                    │  (enable/disable/     │
                    │   check items)        │
                    └───────────────────────┘
```

### Stateful Menu Items

| Item | Enabled When | Checked When |
|------|--------------|--------------|
| Start Server | Model selected + llama installed + server not running | — |
| Stop Server | Server running for selected model | — |
| Restart Server | Server running for selected model | — |
| Install llama.cpp | llama NOT installed | — |
| Proxy Toggle | Always | Proxy running |
| Copy Proxy URL | Proxy running | — |
| Open Proxy | Proxy running | — |

## Menu Event Flow

When a user clicks a menu item, it triggers a Tauri event that the React frontend handles.

```text
User clicks "Start Server"
          │
          ▼
    ┌─────────────────────┐
    │  handle_menu_event  │
    │  (handlers.rs)      │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  emit_or_log(       │
    │    "menu:start-     │
    │     server"         │
    │  )                  │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │  React Frontend     │
    │  Listens for event  │
    │  → calls invoke()   │
    │  → serve_model cmd  │
    └─────────────────────┘
```

<!-- module-docs:end -->
