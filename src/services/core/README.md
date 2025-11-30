# AppCore Service Layer

<!-- module-docs:start -->

The `core` module provides a unified service layer for gglib that can be used by both CLI and GUI interfaces.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────────────┐
│                              AppCore                                     │
│         (Facade holding SqlitePool + service accessors)                  │
├─────────────┬─────────────┬─────────────┬──────────────┬────────────────┤
│ ModelService│ServerService│ProxyService │SettingsServ │HuggingFaceSvc  │
│             │             │             │              │                │
│ - list()    │ - start()   │ - start()   │ - get()      │ - search()     │
│ - get_by_id │ - stop()    │ - stop()    │ - update()   │ - get_quants() │
│ - add()     │ - list()    │ - status()  │ - validate() │ - fetch_tree() │
│ - update()  │ - get()     │             │              │                │
│ - remove()  │             │             │              │                │
│ - tags...   │             │             │              │                │
└─────────────┴─────────────┴─────────────┴──────────────┴────────────────┘
```

## Usage

### CLI Commands

```rust,ignore
use crate::services::{database, AppCore};

pub async fn handle_command(id: u32) -> Result<()> {
    let pool = database::setup_database().await?;
    let core = AppCore::new(pool);
    
    // Use services through AppCore
    let model = core.models().get_by_id(id).await?;
    println!("Found: {}", model.name);
    
    Ok(())
}
```

### GUI Backend

```rust,ignore
use crate::services::core::AppCore;

pub struct GuiBackend {
    core: AppCore,
    // ... GUI-specific state
}

impl GuiBackend {
    pub async fn list_models(&self) -> Result<Vec<GuiModel>> {
        let models = self.core.models().list().await?;
        // Add GUI-specific data like server status...
    }
}
```

## Services

### ModelService

CRUD operations for GGUF models:
- `list()` - List all models
- `get_by_id(id)` - Get model by database ID
- `find_by_identifier(id_or_name)` - Find by ID or name
- `add(model)` / `add_from_file(path)` - Add model (auto-detects reasoning capabilities)
- `update(id, model)` - Update model
- `remove(id)` - Remove model
- Tag operations: `list_tags()`, `add_tag()`, `remove_tag()`, `get_tags()`, `get_by_tag()`

**Reasoning Detection:** When adding models via `add_from_file()`, the service automatically analyzes GGUF metadata for reasoning/thinking patterns (e.g., `<think>` tags in chat templates, model names like "deepseek-r1"). Detected reasoning models receive a `reasoning` tag for optimal llama-server configuration.

### ServerService

Llama-server lifecycle management (wraps ProcessManager):
- `start(id, name, path, ctx, jinja)` - Start server
- `stop(id)` - Stop server
- `list()` - List running servers
- `get(id)` - Get specific server info
- `update_health()` - Health check all servers

### ProxyService

OpenAI-compatible proxy management:
- `start(host, port, ctx)` - Start proxy
- `stop()` - Stop proxy
- `status()` - Get proxy status

### SettingsService

Application settings:
- `get()` - Get current settings
- `update(update)` - Update settings
- `validate(settings)` - Validate settings
- `get_models_directory()` - Get models directory info

### DownloadService

HuggingFace downloads with queue management:
- `queue_download(repo_id, quant, token)` - Add download to queue
- `queue_sharded_download(repo_id, quant, shards)` - Add sharded model to queue (creates one entry per shard)
- `get_queue_status()` - Get current queue status (pending, active, failed items)
- `remove_from_queue(id)` - Remove pending download from queue
- `remove_shard_group(group_id)` - Remove all shards in a group from queue
- `clear_failed()` - Clear all failed downloads
- `cancel(model_id)` - Cancel active download
- `cancel_shard_group(group_id)` - Cancel active download and remove all related shards
- `is_downloading(model_id)` - Check if model is currently downloading

### HuggingFaceService

HuggingFace Hub API integration for searching and browsing GGUF models:
- `search_models_paginated(request)` - Search models with pagination, filtering, and sorting
- `search_models(query, limit, sort)` - Simple search for CLI usage
- `get_quantizations(model_id)` - Get available quantization variants for a model
- `fetch_tree(model_id, path)` - Fetch repository file tree
- `get_commit_sha(model_id)` - Get latest commit SHA for a model
- `find_gguf_files_for_quantization(model_id, quant)` - Find all GGUF files for a specific quantization

**Search Options:**
- Sort by: `downloads`, `likes`, `lastModified`, `createdAt`, `id` (alphabetical)
- Filter by: parameter count range, search query
- Pagination with configurable page size

**Example:**
```rust,ignore
let core = AppCore::new(pool);
let request = HfSearchRequest {
    query: Some("llama".to_string()),
    sort_by: HfSortField::Likes,
    sort_ascending: false,
    ..Default::default()
};
let results = core.huggingface().search_models_paginated(request).await?;
```

#### Sharded Model Support

When downloading sharded models (models split into multiple GGUF files), each shard appears as a separate queue item with linked `group_id` and `ShardInfo`:

```rust,ignore
pub struct ShardInfo {
    pub shard_index: usize,    // 0-based index (0, 1, 2, ...)
    pub total_shards: usize,   // Total count (e.g., 3)
    pub filename: String,      // Shard filename
}
```

- Shards download sequentially (one at a time)
- Cancelling or failing any shard cancels the entire group
- Failed sharded models appear as a single retry entry
- On retry, only missing shards are re-downloaded

## Design Principles

1. **Separation of Concerns**: Each service handles one domain
2. **No Interactive Prompts**: Services receive complete data; CLI handles user interaction
3. **Consistent Error Handling**: All methods return `Result<T>`
4. **Clone-friendly**: Services are cheap to clone (hold Arc/Pool references)
5. **Async-first**: All I/O operations are async

## Migration from Direct Database Calls

Before:
```rust,ignore
let pool = database::setup_database().await?;
let model = database::get_model_by_id(&pool, id).await?;
```

After:
```rust,ignore
let pool = database::setup_database().await?;
let core = AppCore::new(pool);
let model = core.models().get_by_id(id).await?;
```

<!-- module-docs:end -->
