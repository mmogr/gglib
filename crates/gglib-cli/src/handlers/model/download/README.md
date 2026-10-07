# download

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-download-complexity.json)

<!-- module-docs:start -->

`HuggingFace` Hub download command handlers for the CLI.

## Purpose

This module handles all download-related commands that interact with `HuggingFace` Hub, including searching for models, browsing popular models, downloading GGUF files with interactive queue management, checking for updates, and updating existing models.

## Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│                    download Module                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  model download → exec.rs ──► daemon queue ──► remote.rs         │
│  gglib up ──► DownloadManagerPort ──► interactive.rs             │
│                                          ↕  [a]/[q] hotkeys      │
│        both read a QueueSnapshot every 250 ms, and hand it to    │
│                                                                  │
│        monitor.rs  QueueWatch::take     draw it; have they ended?│
│                    MonitorState::step   the one rule for that    │
│        board.rs    DownloadBoard::sync  one line per download    │
│                                                                  │
│  model upgrade → update_model.rs ──► board.rs SoloBoard, its row │
│                          ↓                                       │
│        crate::console::CliConsole                                │
│        (indicatif MultiProgress, stderr, footer-pinned hint,     │
│         console hook for gglib_core::telemetry)                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Key Flow:**
1. `gglib model download <repo>`: `exec.rs` queues the download on the gglib
   daemon, which answers with the download's ID, and `remote.rs` polls the
   daemon's queue for that ID. `gglib up` queues on this
   process's own download manager and runs `interactive.rs`. Neither subscribes
   to events: each reads the queue snapshot four times a second.
2. `board.rs` draws the snapshot, one line per download, from the row's own
   text (`DownloadRowText`), which is the text the GUI shows. The line leaves
   out one word of it: the status of a plain transfer, `Downloading`
   (`STATUS_DOWNLOADING`), whose numbers say as much. On a terminal a
   row is a bar, `name · file [bar] numbers`, kept from the download's first
   file to its last; the file reads `part 2/3`, `weights` or `projector`. The
   bar's fill is the row's `percent`, as the GUI's is.
   When **stderr** is not a terminal a row is a plain, undated line, printed
   every two seconds. TTY detection and the `console::Term` used for hotkeys
   and the `[a]` prompt check stderr too, so a redirected stdout doesn't
   silently disable either.
3. `monitor.rs` holds the one rule for when a monitor is done,
   `MonitorState::step`. `remote.rs` and both loops of `interactive.rs` reach
   it through `QueueWatch::take`, which draws the snapshot, takes the step and
   prints the outcomes. A monitor goes on while a download of its own is a
   row: running, between two of its files, or waiting. It exits on its own
   outcome in the snapshot's finished list, which covers a download that
   ended before the first poll. The daemon monitor follows one download, the
   ID the daemon answered its queue request with: another quantization of
   the same repository is not its own, and neither is that one's outcome. The
   in-process monitor counts every download.
4. How each download ended is printed when its monitor exits that way: a
   `✓` or `✗` and the finished entry's own `text`, which is the text the GUI's
   toast shows. The daemon monitor exits non-zero when its download failed or
   was cancelled, with that text as its error, and a model the library
   refused is a failure. Every download leaves an outcome, one removed
   while it waited included. The monitor also exits non-zero when its
   download's outcome is gone from the queue, cleared or pushed out by later
   ones. A forced quit of the in-process monitor prints no outcomes.
5. In TTY mode, `interactive.rs`: `[a]` prompts for another model to add to
   the queue. `[q]` (or `Esc` / `Ctrl-C`) is **two-step**:
   - First press → arms drain mode (hint becomes
     `Draining... press q again to force quit`); active downloads
     continue running until they finish naturally and the queue auto-
     exits when it empties.
   - Second press → calls `cancel_all()`, which ends each waiting
     download as cancelled, signals the cancel token of the transfer in
     flight, and waits up to 5 s for in-flight Python helpers to actually
     finalize before returning.

   The `[a]/[q]` hint bar is created eagerly and registered as the console's
   *footer* (`CliConsole::set_footer`); every download bar is
   inserted above it via `MultiProgress::insert_before`, so it stays pinned
   to the bottom without ever being removed and re-added.
6. What a row says it is doing:
   `Downloading` → `Finalizing…` (gathering HF metadata) →
   `Registering…` (writing model row). The last two keep the line from
   looking frozen at 100 %. A note from the transfer (e.g.
   "preparing fast downloader…") stands in for the status until bytes
   arrive, covering the tens of seconds the fast downloader's Python venv
   can take to build.
7. Each file is counted once, whichever transport moves it: the bytes on
   disk fill the bar. See
   [`gglib-download/src/executor/progress.rs`](../../../../../gglib-download/src/executor/progress.rs).
8. Model registration on completion is handled by the download manager
   (via `ModelRegistrarPort`).
9. `CliConsole` installs itself as the process-wide
   console hook (`gglib_core::telemetry::set_console_hook`): any `tracing`
   log line emitted while bars are live is routed through
   `MultiProgress::println` instead of a raw write, so it can't desync the
   bars' redraw bookkeeping and strand old frames in scrollback.
10. `gglib model upgrade` is the one download that goes through no queue:
    `ModelOps::apply_upgrade` fetches the files in this process. There is no
    snapshot to read, so it hands over the download's row, made by the row
    builder the queue uses, and `update_model.rs` draws it with `SoloBoard`
    (`board.rs`), which shows it on a `DownloadBoard` as the queue's running
    download. The line is the one a queued download of the same files would
    have, on a terminal and piped alike.

## Commands

### `search`
Search `HuggingFace` Hub for GGUF models.

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

### `download` (exec + remote)
Download a model from `HuggingFace` Hub, on the gglib daemon.

**Module:** `exec.rs` (orchestrator), `remote.rs` (daemon queue monitor);
`interactive.rs` is the in-process monitor `gglib up` uses

**Options:**
- `--quantization <QUANT>` / `-q` - Specific quantization (e.g., "`Q4_K_M`")
- `--list-quants` - List available quantizations, then the repository's projectors with their sizes, each marked with the quantizations whose download fetches it (uses `--token` if provided)
- `--token <TOKEN>` - `HuggingFace` token (for `--list-quants` only; use `HF_TOKEN` env var for downloads)
- `--skip-db` - Accepted and reported as not honoured: registration happens daemon-side

**Interactive mode (TTY, the in-process monitor):**
- `[a]` — add another model to the queue while a download is running
- `[q]` / Ctrl-C — cancel all pending downloads and exit cleanly
- Falls back to a plain polling monitor when **stderr** is not a TTY (CI, pipes) —
  stderr, not stdout, since that's where the bars themselves draw

**Flow:**
1. Queue the model on the daemon (`POST /api/models/downloads/queue`, the route the GUI uses)
2. Poll the daemon's queue snapshot and draw it on the download board
3. Exit when the model's download has ended, printing how
4. On completion, model is registered automatically (via `ModelRegistrarPort`)

A repository that has projectors gets one fetched with the model, and the model is linked to it: the projector of the download's own quantization, else the `F16` one, else the first by name. There is no flag to leave it out; `gglib model update <model> --no-projector` unlinks it afterwards. The queue monitor names its file `projector`.

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
Check if downloaded models have updates on `HuggingFace` Hub. Sends `HF_TOKEN` to
the Hub when it is set.

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
Update a model to the latest version from `HuggingFace` Hub.

**Module:** `update_model.rs`

**Options:**
- `--force` - Skip confirmation prompt

**Flow:**
1. Check if model has `HuggingFace` source
2. Query Hub for latest version
3. Download new version, drawn as one line on the download board
4. Replace old file
5. Update database metadata

**Example:**
```bash
gglib model upgrade 1
gglib model upgrade 1 --force
```

## Architecture Details

### Download Execution
`model download` and `gglib up` queue on a download manager, the daemon's or
this process's own, and draw its snapshots. `model upgrade` fetches with
`gglib_download::cli_exec::update_model` and draws the row that hands over.
Every model download's line on the terminal is drawn by `board.rs`.

### Database Integration
After successful download:
1. Parse GGUF metadata via `GgufParserPort`
2. Create `NewModel` entity
3. Call `ModelRepository::add_model()`
4. Display confirmation with model ID

### Error Handling
Handlers convert download errors to user-friendly messages:
- Network errors → "Failed to connect to `HuggingFace` Hub"
- Invalid repo → "Repository not found or private"
- Parse errors → "Invalid GGUF file downloaded"
- Database errors → "Failed to register model"

## Dependencies

- **gglib-download** - Core download functionality via `cli_exec`
- **gglib-hf** - `HuggingFace` Hub client
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
