<!-- module-docs:start -->

# Commands Module

The commands module contains Tauri command handlers for **OS integration only**. All business logic is served via the HTTP API.

## Architecture (HTTP-First)

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                         React Frontend                                   │
│                                                                          │
│  HTTP API (primary)           Tauri IPC (OS integration only)           │
│  ────────────────             ────────────────────────────────           │
│  GET /api/models              invoke("get_embedded_api_info")           │
│  POST /api/servers/start      invoke("open_url")                        │
│  GET /api/events (SSE)        invoke("set_selected_model")              │
│                               invoke("check_llama_status")               │
└──────────┬──────────────────────────────┬───────────────────────────────┘
           │                              │
           ▼                              ▼
┌─────────────────────────┐  ┌─────────────────────────────────────────┐
│   Embedded Axum Server  │  │        Tauri IPC Layer                  │
│   (Bearer auth)         │  │                                         │
│  - /api/models          │  │  ┌──────────┐  ┌──────────┐            │
│  - /api/servers         │  │  │  util.rs │  │ llama.rs │            │
│  - /api/downloads       │  │  │ (4 cmds) │  │ (2 cmds) │            │
│  - /api/chat            │  │  └─────┬────┘  └─────┬────┘            │
│  - /api/proxy           │  └────────┼─────────────┼─────────────────┘
│  - /api/mcp             │           │             │
│  - /api/events (SSE)    │           └─────────────┘
└────────┬────────────────┘                   │
         │                                    │
         └────────────────────────────────────┘
                           │
                           ▼
              ┌─────────────────────────┐
              │       AppState          │
              │  ┌───────────────────┐  │
              │  │    GuiBackend     │  │
              │  │  (shared service) │  │
              │  └─────────┬─────────┘  │
              └────────────┼────────────┘
                           │
                           ▼
              ┌─────────────────────────┐
              │   gglib-axum crate      │
              │  (HTTP handlers with    │
              │   Bearer auth)          │
              └─────────────────────────┘
```

## Command Inventory (6 total)

**Current Surface**: 6 OS integration commands. Business logic served via HTTP API.

| Module | Purpose | Commands |
|--------|---------|----------|
| **util.rs** | API discovery | `get_embedded_api_info` |
| **util.rs** | Shell integration | `open_url` |
| **util.rs** | Menu sync | `set_selected_model`, `sync_menu_state` |
| **llama.rs** | Binary management | `check_llama_status`, `install_llama` |

### Business Logic via HTTP

The following functionality is served via HTTP API endpoints:
- `/api/models` - Model CRUD operations
- `/api/servers` - llama-server lifecycle management
- `/api/downloads` - Download queue and progress
- `/api/huggingface` - HuggingFace Hub search and metadata
- `/api/tags` - Model tagging and filtering
- `/api/proxy` - OpenAI-compatible proxy control
- `/api/settings` - User preferences
- `/api/mcp` - MCP server management
- `/api/chat` - Chat history and conversations

## Command Pattern

Commands are **thin OS integration wrappers only**:

```rust
#[tauri::command]
pub async fn get_embedded_api_info(
    state: tauri::State<'_, AppState>,
) -> Result<EmbeddedApiInfo, String> {
    Ok(EmbeddedApiInfo {
        port: state.embedded_api.port,
        token: state.embedded_api.auth_token.clone(),
        base_url: state.embedded_api.base_url.clone(),
    })
}
```

### Design Rules

1. **OS integration only** — Commands limited to platform-specific operations (file system, shell, menu, native binaries)
2. **HTTP-first architecture** — All business logic served via `/api/*` endpoints with Bearer auth
3. **No business logic** — Commands do NOT access GuiBackend for domain operations
4. **Policy enforcement** — CI gates (`scripts/check-tauri-commands.sh`, `scripts/check-frontend-ipc.sh`) prevent unauthorized commands

### Command Allowlist

Only these 6 commands are permitted:

**API Discovery**:
- `get_embedded_api_info` - Returns `{ port, token, base_url }` for HTTP client setup

**Shell Integration**:
- `open_url` - Opens URLs in system browser (OS-specific)

**Menu Synchronization** (macOS native menus):
- `set_selected_model` - Updates selected model in app state for menu badge
- `sync_menu_state` - Rebuilds native menu with current state

**Binary Management** (llama.cpp installation):
- `check_llama_status` - Checks if llama.cpp binaries are installed
- `install_llama` - Downloads and installs llama.cpp from GitHub releases

Any new command must justify OS integration necessity and pass CI gate review.

## Request Flow

**Business Logic** (via HTTP API):
```text
Frontend: fetch("/api/models")
                    │
                    ▼
┌─────────────────────────────────────────────────────┐
│  Axum HTTP Handler                                  │
│  Authorization: Bearer <token>                      │
│                                                     │
│  pub async fn list_models(                          │
│      State(ctx): State<AxumContext>,                │
│  ) -> Result<Json<Vec<ModelRow>>, StatusCode>      │
└────────────────────────┬────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│  ctx.gui.list_models().await                        │
│                                                     │
│  GuiBackend:                                        │
│  - Queries database                                 │
│  - Returns model list                               │
└────────────────────────┬────────────────────────────┘
                         │
                         ▼
                Frontend receives JSON
```

**OS Integration** (via Tauri IPC):
```text
Frontend: invoke("open_url", { url: "https://..." })
                    │
                    ▼
┌─────────────────────────────────────────────────────┐
│  #[tauri::command]                                  │
│  pub async fn open_url(url: String) -> Result<()>  │
│                                                     │
│  - macOS: open command                              │
│  - Windows: start command                           │
│  - Linux: xdg-open                                  │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
                System browser opens
```

## CI Gates

Two scripts enforce HTTP-first architecture:

**`scripts/check-tauri-commands.sh`**:
- Validates `#[tauri::command]` only in util.rs and llama.rs
- Ensures commands/ directory contains only mod.rs, util.rs, llama.rs
- Checks invoke_handler! only registers 6 allowed commands
- Prevents addition of business logic commands

**`scripts/check-frontend-ipc.sh`**:
- Validates all `invoke()` calls against OS integration allowlist
- Permits internal helpers in platform/tauri.ts and api/client.ts  
- Prevents frontend from bypassing HTTP API via direct IPC

Run gates locally:
```bash
./scripts/check-tauri-commands.sh
./scripts/check-frontend-ipc.sh
```

Both must pass before merging changes to commands/.

<!-- module-docs:end -->
