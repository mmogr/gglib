# AppCore Service Layer

<!-- module-docs:start -->

The `core` module provides a unified service layer for gglib that can be used by both CLI and GUI interfaces.

## Architecture

```text
┌─────────────────────────────────────────────────────────┐
│                        AppCore                          │
│  (Facade holding SqlitePool + service accessors)        │
├─────────────┬─────────────┬─────────────┬──────────────┤
│ ModelService│ServerService│ProxyService │SettingsService│
│             │             │             │              │
│ - list()    │ - start()   │ - start()   │ - get()      │
│ - get_by_id │ - stop()    │ - stop()    │ - update()   │
│ - add()     │ - list()    │ - status()  │ - validate() │
│ - update()  │ - get()     │             │              │
│ - remove()  │             │             │              │
│ - tags...   │             │             │              │
└─────────────┴─────────────┴─────────────┴──────────────┘
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
- `add(model)` / `add_from_file(path)` - Add model
- `update(id, model)` - Update model
- `remove(id)` - Remove model
- Tag operations: `list_tags()`, `add_tag()`, `remove_tag()`, `get_tags()`, `get_by_tag()`

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
- `get_queue_status()` - Get current queue status (pending, active, failed items)
- `remove_from_queue(id)` - Remove pending download from queue
- `clear_failed()` - Clear all failed downloads
- `cancel(model_id)` - Cancel active download
- `is_downloading(model_id)` - Check if model is currently downloading

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
