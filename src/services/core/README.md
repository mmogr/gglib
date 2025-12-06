# AppCore Service Layer

<!-- module-docs:start -->

The `core` module provides a unified service layer for gglib that can be used by both CLI and GUI interfaces.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────────────┐
│                              AppCore                                     │
│         (Facade holding SqlitePool + service accessors)                  │
├─────────────┬─────────────┬─────────────┬──────────────┬────────────────┤
│ ModelService│ServerService│ProxyService │SettingsServ │HuggingfaceClnt │
│             │             │             │              │                │
│ - list()    │ - start()   │ - start()   │ - get()      │ - search()     │
│ - get_by_id │ - stop()    │ - stop()    │ - update()   │ - get_quants() │
│ - add()     │ - list()    │ - status()  │ - validate() │ - get_sha()    │
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

### Downloads (via DownloadManager)

Downloads are handled by `DownloadManager` from the [`src/download/`](../../download/README.md) module.
Access via `core.downloads()`:

- `queue_download_auto(repo_id, quant)` - Queue download (auto-detects shards)
- `get_queue_snapshot()` - Get current queue state (pending, active, failed)
- `active_downloads()` - List currently downloading items
- `cancel(id)` - Cancel an active download
- `remove_from_queue(id)` - Remove pending download
- `reorder_queue(id, position)` - Move item to new queue position
- `cancel_shard_group(group_id)` - Cancel all shards in a group
- `clear_failed()` - Clear all failed downloads
- `retry_failed_download(id)` - Re-queue a failed download

### HuggingfaceClient

HuggingFace Hub API integration for searching and browsing GGUF models.
See the [`gglib-hf`](../../../crates/gglib-hf/README.md) crate for the full implementation.
Uses the `HfClientPort` trait from `gglib-core` for abstraction.

- `search_models_page(query)` - Search models with pagination, filtering, and sorting
- `list_quantizations(repo)` - Get available quantization variants for a model
- `find_quantization_files(repo, quant)` - Find all GGUF files for a specific quantization
- `get_commit_sha(repo)` - Get latest commit SHA for a model
- `get_tool_support(repo)` - Check if model supports function calling

**Example:**
```rust,ignore
use gglib_core::ports::huggingface::{HfClientPort, HfSearchOptions};
use gglib_hf::DefaultHfClient;

let core = AppCore::new(pool);
let options = HfSearchOptions {
    query: Some("llama".to_string()),
    ..Default::default()
};
let results = core.huggingface().search(&options).await?;
```

**Example:**
```rust,ignore
let (id, position, shard_count) = core.downloads()
    .queue_download_auto("TheBloke/Llama-2-7B-GGUF", "Q4_K_M")
    .await?;
```

#### Sharded Model Support

When downloading sharded models (models split into multiple GGUF files), `queue_download_auto()` automatically detects shards and creates linked queue items with shared `group_id` and `ShardInfo`:

```rust,ignore
pub struct ShardInfo {
    pub shard_index: u32,      // 0-based index (0, 1, 2, ...)
    pub total_shards: u32,     // Total count (e.g., 3)
    pub filename: String,      // Shard filename
    pub file_size: Option<u64>,// Size in bytes (for aggregate progress)
}
```

- Shards download sequentially (one at a time)
- Cancelling or failing any shard cancels the entire group
- Progress events include aggregate totals across all shards
- On retry, the download is re-queued with fresh shard detection

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
