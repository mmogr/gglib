# Database Module

<!-- module-docs:start -->

The `database` module handles all SQLite database interactions for GGUF model storage and management.

## Architecture

```text
┌────────────────────────────────────────────────────────────────┐
│                        database/mod.rs                         │
│                  (setup_database, re-exports)                  │
├─────────────┬───────────┬─────────────┬─────────────┬──────────┤
│ schema.rs   │ models.rs │ tags.rs     │ from_row.rs │ error.rs │
│             │           │             │             │          │
│ DDL         │ CRUD for  │ Tag         │ FromRow     │ Domain   │
│  migrations │  models   │  operations │  impls      │  errors  │
└─────────────┴───────────┴─────────────┴─────────────┴──────────┘
```

## Submodules

- **`schema.rs`**: Database schema creation and migrations (`create_schema`, `ensure_column_exists`)
- **`models.rs`**: Model CRUD operations (`add_model`, `list_models`, `get_model_by_id`, `update_model`, `remove_model_by_id`)
- **`tags.rs`**: Tag management (`list_tags`, `add_model_tag`, `remove_model_tag`, `get_model_tags`, `get_models_by_tag`)
- **`from_row.rs`**: SQLx `FromRow` trait implementation for `Gguf`
- **`error.rs`**: Domain-specific error types (`ModelStoreError`)

## Usage

### Setting Up the Database

```rust,ignore
use gglib::services::database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = database::setup_database().await?;
    // Use pool for database operations...
    Ok(())
}
```

### Model Operations

```rust,ignore
use gglib::services::database;
use gglib::models::Gguf;

// List all models
let models = database::list_models(&pool).await?;

// Get by ID
let model = database::get_model_by_id(&pool, 1).await?;

// Find by identifier (ID or name)
let model = database::find_model_by_identifier(&pool, "my-model").await?;

// Add a model
let id = database::add_model(&pool, &gguf_model).await?;

// Update a model (returns error if not found)
database::update_model(&pool, id, &updated_model).await?;

// Remove a model
database::remove_model_by_id(&pool, id).await?;
```

### Tag Operations

```rust,ignore
use gglib::services::database;

// List all unique tags
let tags = database::list_tags(&pool).await?;

// Add a tag to a model
database::add_model_tag(&pool, model_id, "coding".to_string()).await?;

// Get tags for a model
let model_tags = database::get_model_tags(&pool, model_id).await?;

// Get all models with a specific tag
let model_ids = database::get_models_by_tag(&pool, "coding".to_string()).await?;

// Remove a tag from a model
database::remove_model_tag(&pool, model_id, "coding".to_string()).await?;
```

## Error Handling

The module uses `ModelStoreError` for domain-specific errors:

```rust,ignore
use gglib::services::database::ModelStoreError;

match database::update_model(&pool, id, &model).await {
    Ok(()) => println!("Updated!"),
    Err(e) => {
        if let Some(ModelStoreError::NotFound { id }) = e.downcast_ref() {
            println!("Model {} not found", id);
        }
    }
}
```

### Error Variants

| Variant | Description |
|---------|-------------|
| `DuplicateModel` | Model with same name/path already exists |
| `NotFound { id }` | Model with specified ID not found |
| `Database(sqlx::Error)` | Underlying database error |
| `Serialization(serde_json::Error)` | JSON serialization/deserialization error |

## Schema

The module manages the following tables:

### `models` Table

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER | Primary key (auto-increment) |
| `name` | TEXT | Model display name |
| `file_path` | TEXT | Absolute path to GGUF file (unique) |
| `param_count_b` | REAL | Parameter count in billions |
| `architecture` | TEXT | Model architecture (llama, mistral, etc.) |
| `quantization` | TEXT | Quantization format (e.g., Q4_K_M) |
| `context_length` | INTEGER | Maximum context length |
| `metadata` | TEXT | JSON blob with additional GGUF metadata |
| `added_at` | TEXT | ISO8601 timestamp when added |
| `hf_repo_id` | TEXT | Optional HuggingFace repository ID |
| `hf_commit_sha` | TEXT | Optional HuggingFace commit SHA |
| `hf_filename` | TEXT | Optional HuggingFace filename |
| `download_date` | TEXT | Optional download timestamp |
| `last_update_check` | TEXT | Optional last update check timestamp |
| `tags` | TEXT | JSON array of tag strings (default: '[]') |

### Indexes

- `idx_models_file_path` - Unique index on file path (prevents duplicates)
- `idx_models_name` - Fast lookup by name

## Testing

For testing, use the shared test infrastructure in `tests/common/`:

```rust,ignore
use crate::common::database::setup_test_pool;
use crate::common::fixtures::{create_test_model, create_test_model_with_params};

#[tokio::test]
async fn test_example() {
    let pool = setup_test_pool().await.unwrap();
    
    // Basic model with defaults (7B params)
    let model = create_test_model("test-model");
    
    // Model with custom param count
    let large_model = create_test_model_with_params("large-model", 70.0);
    
    database::add_model(&pool, &model).await.unwrap();
    // ... assertions
}
```

The test pool uses the production `create_schema()` function to ensure schema parity between tests and production.

<!-- module-docs:end -->