<!-- module-docs:start -->

# Commands Module

The commands module contains all Tauri command handlers, organized into domain-specific submodules. Commands are thin wrappers that delegate to the shared `GuiBackend` service.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                         React Frontend                                   │
│                                                                          │
│  invoke("list_models")  invoke("serve_model")  invoke("start_proxy")    │
└──────────┬────────────────────┬─────────────────────┬───────────────────┘
           │                    │                     │
           ▼                    ▼                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                      Tauri IPC Layer                                     │
└──────────┬────────────────────┬─────────────────────┬───────────────────┘
           │                    │                     │
           ▼                    ▼                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         commands/                                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ │
│  │  models   │ │  servers  │ │ downloads │ │   proxy   │ │ settings  │ │
│  │  .rs      │ │  .rs      │ │  .rs      │ │  .rs      │ │  .rs      │ │
│  └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ │
│        │             │             │             │             │        │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ │
│  │huggingface│ │   tags    │ │   llama   │ │    mcp    │ │   util    │ │
│  │  .rs      │ │  .rs      │ │  .rs      │ │  .rs      │ │  .rs      │ │
│  └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ │
│        │             │             │             │             │        │
└────────┼─────────────┼─────────────┼─────────────┼─────────────┼────────┘
         │             │             │             │             │
         └─────────────┴─────────────┼─────────────┴─────────────┘
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
                        │      gglib crate        │
                        │  (Database, Proxy,      │
                        │   Download, HuggingFace)│
                        └─────────────────────────┘
```

## Command Modules

| Module | Domain | Commands |
|--------|--------|----------|
| **models.rs** | Model CRUD | `list_models`, `add_model`, `remove_model`, `update_model` |
| **servers.rs** | llama-server lifecycle | `serve_model`, `stop_server`, `list_servers`, `get_server_logs`, `clear_server_logs` |
| **downloads.rs** | Download management | `download_model`, `cancel_download`, `queue_download`, `get_download_queue`, `remove_from_download_queue`, `reorder_download_queue`, `cancel_shard_group`, `clear_failed_downloads` |
| **huggingface.rs** | HuggingFace API | `browse_hf_models`, `get_hf_quantizations`, `get_hf_tool_support`, `search_models` |
| **tags.rs** | Model tagging | `list_tags`, `get_model_filter_options`, `add_model_tag`, `remove_model_tag`, `get_model_tags` |
| **proxy.rs** | OpenAI-compatible proxy | `start_proxy`, `stop_proxy`, `get_proxy_status` |
| **settings.rs** | User preferences | `get_settings`, `update_settings` |
| **llama.rs** | llama.cpp installation | `check_llama_status`, `install_llama` |
| **mcp.rs** | MCP server management | `list_mcp_servers`, `add_mcp_server`, `remove_mcp_server`, `update_mcp_server`, `toggle_mcp_server`, `get_mcp_server_status`, `test_mcp_server`, `refresh_mcp_tools` |
| **util.rs** | Utilities | `open_url`, `get_gui_api_port`, `set_selected_model`, `sync_menu_state` |

## Command Pattern

All commands follow a consistent thin-wrapper pattern:

```rust
#[tauri::command]
pub async fn command_name(
    state: tauri::State<'_, AppState>,
    // ... additional parameters
) -> Result<ReturnType, String> {
    state.backend
        .some_method(/* args */)
        .await
        .map_err(|e| format!("Failed to do X: {}", e))
}
```

### Design Rules

1. **Thin wrappers only** — Commands delegate to `GuiBackend`; no business logic.
2. **Import only `crate::app`** — Commands import `AppState` from `crate::app`, not from each other.
3. **No cross-command imports** — Each command module is independent.
4. **Domain types stay local** — Types like `QueueDownloadResponse` or `LlamaStatus` are defined in their respective command modules.
5. **Errors as strings** — Tauri commands return `Result<T, String>` for frontend compatibility.

## Request Flow

```text
Frontend: invoke("serve_model", { modelId: 42 })
                    │
                    ▼
┌─────────────────────────────────────────────────────┐
│  #[tauri::command]                                  │
│  pub async fn serve_model(                          │
│      state: tauri::State<'_, AppState>,             │
│      model_id: u32,                                 │
│  ) -> Result<ServerInfo, String>                    │
└────────────────────────┬────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│  state.backend.start_server(model_id).await         │
│                                                     │
│  GuiBackend:                                        │
│  - Finds model in database                          │
│  - Checks for free port                             │
│  - Spawns llama-server process                      │
│  - Returns ServerInfo                               │
└────────────────────────┬────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│  .map_err(|e| format!("Failed to start: {}", e))    │
│                                                     │
│  → Ok(ServerInfo { port, pid, ... })                │
│  → Err("Failed to start: model not found")         │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
                Frontend receives result
```

<!-- module-docs:end -->
