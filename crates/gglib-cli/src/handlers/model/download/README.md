# download

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-complexity.json)

<!-- module-docs:start -->

HuggingFace Hub download command handlers for the CLI.

## Purpose

This module handles all download-related commands that interact with HuggingFace Hub, including searching for models, browsing popular models, downloading GGUF files with interactive queue management, checking for updates, and updating existing models.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│                    download Module                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  CLI → exec.rs ──queue_smart()──► DownloadManagerPort           │
│              └───────────────────► interactive.rs (TUI monitor) │
│                                        ↕  [a]/[q] hotkeys        │
│                                   CliDownloadEventEmitter        │
│                                   (indicatif MultiProgress)      │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Key Flow:**
1. User issues `gglib model download <repo>` command
2. `exec.rs` queues it via `DownloadManagerPort::queue_smart` (same code path as the GUI)
3. `interactive.rs` renders progress via `CliDownloadEventEmitter` (indicatif bars)
4. In TTY mode: `[a]` prompts for another model to add to the queue; `[q]` cancels all
5. Model registration on completion is handled by the download manager (via `ModelRegistrarPort`)

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`browse.rs`](browse.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-browse-coverage.json) |
| [`check_updates.rs`](check_updates.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-check_updates-coverage.json) |
| [`exec.rs`](exec.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-exec-coverage.json) |
| [`interactive.rs`](interactive.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-interactive-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-interactive-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-interactive-coverage.json) |
| [`search.rs`](search.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-search-coverage.json) |
| [`update_model.rs`](update_model.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-update_model-coverage.json) |
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
gglib model search "llama 7b" --limit 5 --sort downloads
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
gglib model browse popular --limit 10
gglib model browse recent --size 7B
```

### `download` (exec + interactive)
Download a model from HuggingFace Hub with interactive queue support.

**Module:** `exec.rs` (orchestrator), `interactive.rs` (TUI monitor)

**Options:**
- `--quantization <QUANT>` / `-q` - Specific quantization (e.g., "Q4_K_M")
- `--list-quants` - List available quantizations (uses `--token` if provided)
- `--token <TOKEN>` - HuggingFace token (for `--list-quants` only; use `HF_TOKEN` env var for downloads)
- `--force` / `-f` - Skip confirmation prompt

**Interactive mode (TTY):**
- `[a]` — add another model to the queue while a download is running
- `[q]` / Ctrl-C — cancel all pending downloads and exit cleanly
- Falls back to a plain polling monitor when stdout is not a TTY (CI, pipes)

**Flow:**
1. Queue initial model via `DownloadManagerPort::queue_smart` (same path as GUI)
2. Enter the interactive monitor loop
3. Download manager handles progress events → `CliDownloadEventEmitter` renders indicatif bars
4. On completion, model is registered automatically (via `ModelRegistrarPort`)

**Example:**
```bash
# List available quantizations
gglib model download microsoft/DialoGPT-medium --list-quants

# Download specific quantization — enters live queue monitor
gglib model download microsoft/DialoGPT-medium -q Q4_K_M

# Download with HF token for private repos (set env var for downloads)
HF_TOKEN=hf_... gglib model download my-org/private-model -q Q4_K_M
```

### `check-updates`
Check if downloaded models have updates on HuggingFace Hub.

**Module:** `check_updates.rs`

**Options:**
- `--model-id <ID>` - Check specific model
- `--all` - Check all models

**Example:**
```bash
gglib model check-updates --all
gglib model check-updates --model-id 1
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
gglib model upgrade 1
gglib model upgrade 1 --force
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
