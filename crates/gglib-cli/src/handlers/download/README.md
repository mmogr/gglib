# download

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-complexity.json)

<!-- module-docs:start -->

HuggingFace Hub download command handlers for the CLI.

## Purpose

This module handles all download-related commands that interact with HuggingFace Hub, including searching for models, browsing popular models, downloading GGUF files, checking for updates, and updating existing models.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│                    download Module                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  CLI → Handler → cli_exec → HF Client → Database Registration   │
│       (this)     (download) (hf crate)   (db crate)             │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Key Flow:**
1. User issues download command via CLI
2. Handler validates and prepares arguments
3. Delegates to `gglib-download::cli_exec` for actual download
4. Automatically registers downloaded model in database (unless `--skip-db`)
5. Displays progress and confirmation

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`browse.rs`](browse) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-coverage.json) |
| [`check_updates.rs`](check_updates) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-coverage.json) |
| [`exec.rs`](exec) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-coverage.json) |
| [`search.rs`](search) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-coverage.json) |
| [`update_model.rs`](update_model) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-coverage.json) |
<!-- module-table:end -->

## Commands

### `search`
Search HuggingFace Hub for GGUF models.

**Module:** `search.rs`

**Options:**
- `--limit <N>` - Maximum results (default: 10)
- `--sort <FIELD>` - Sort by "downloads", "created", "likes", or "updated"
- `--gguf-only` - Only show models with GGUF files

**Example:**
```bash
gglib search "llama 7b" --limit 5 --sort downloads
```

### `browse`
Browse popular GGUF models by category.

**Module:** `browse.rs`

**Categories:**
- `popular` - Most popular models
- `recent` - Recently updated models  
- `trending` - Trending models

**Options:**
- `--limit <N>` - Maximum results (default: 20)
- `--size <SIZE>` - Filter by model size (e.g., "7B", "13B")

**Example:**
```bash
gglib browse popular --limit 10
gglib browse recent --size 7B
```

### `download` (exec)
Download a model from HuggingFace Hub.

**Module:** `exec.rs`

**Options:**
- `--quantization <QUANT>` / `-q` - Specific quantization (e.g., "Q4_K_M")
- `--list-quants` - List available quantizations
- `--skip-db` - Skip database registration
- `--token <TOKEN>` - HuggingFace token for private models
- `--force` / `-f` - Skip confirmation prompt

**Flow:**
1. Query HuggingFace Hub for repo info
2. Show available quantizations (if `--list-quants`)
3. Download GGUF file to models directory
4. Parse GGUF metadata
5. Register in database (unless `--skip-db`)

**Example:**
```bash
# List available quantizations
gglib download microsoft/DialoGPT-medium --list-quants

# Download specific quantization
gglib download microsoft/DialoGPT-medium -q Q4_K_M

# Download without database registration
gglib download microsoft/DialoGPT-medium -q Q4_K_M --skip-db
```

### `check-updates`
Check if downloaded models have updates on HuggingFace Hub.

**Module:** `check_updates.rs`

**Options:**
- `--model-id <ID>` - Check specific model
- `--all` - Check all models

**Example:**
```bash
gglib check-updates --all
gglib check-updates --model-id 1
```

### `update-model`
Update a model to the latest version from HuggingFace Hub.

**Module:** `update_model.rs`

**Options:**
- `--force` - Skip confirmation prompt

**Flow:**
1. Check if model has HuggingFace source
2. Query Hub for latest version
3. Download new version
4. Replace old file
5. Update database metadata

**Example:**
```bash
gglib update-model 1
gglib update-model 1 --force
```

## Architecture Details

### Download Execution
Uses `gglib-download::cli_exec::execute_download()` which provides:
- Progress bars via indicatif
- Resumable downloads
- Parallel chunk downloads
- Automatic retry on failure
- Validation of downloaded files

### Database Integration
After successful download:
1. Parse GGUF metadata via `GgufParserPort`
2. Create `NewModel` entity
3. Call `ModelRepository::add_model()`
4. Display confirmation with model ID

### Error Handling
Handlers convert download errors to user-friendly messages:
- Network errors → "Failed to connect to HuggingFace Hub"
- Invalid repo → "Repository not found or private"
- Parse errors → "Invalid GGUF file downloaded"
- Database errors → "Failed to register model"

## Dependencies

- **gglib-download** - Core download functionality via `cli_exec`
- **gglib-hf** - HuggingFace Hub client
- **gglib-db** - Model database operations
- **gglib-gguf** - GGUF metadata parsing
- **gglib-core** - Domain types and ports

## Testing

Tests focus on:
- Argument validation
- Download flow integration
- Database registration
- Error message formatting

Mock external dependencies:
```rust
#[tokio::test]
async fn test_download_with_db_registration() {
    let mut mock_ctx = MockCliContext::new();
    
    mock_ctx.expect_download()
        .returning(|_| Ok(PathBuf::from("/models/model.gguf")));
    
    mock_ctx.expect_register_model()
        .returning(|_| Ok(1));
    
    let args = DownloadArgs {
        repo_id: "test/model".to_string(),
        quantization: Some("Q4_K_M".to_string()),
        skip_db: false,
        ..Default::default()
    };
    
    let result = download::execute(&mock_ctx, args).await;
    assert!(result.is_ok());
}
```

## Design Notes

1. **Thin Handlers** - Delegate heavy lifting to `cli_exec` module
2. **Auto-Registration** - Models registered by default for better UX
3. **Progress Feedback** - All operations show progress indicators
4. **Idempotent Updates** - Safe to re-run update commands
5. **Offline-First** - Check local state before querying Hub when possible

<!-- module-docs:end -->
