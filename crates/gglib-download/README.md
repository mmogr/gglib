# gglib-download

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-complexity.json)

Download queue and manager for `HuggingFace` model files.

Transfers run natively over HTTP — resumable and checksum-verified — so a new
user's first download needs no Python toolchain. The `hf_xet` accelerator is an
opt-in upgrade that falls back to the native path rather than failing.

## Architecture

This crate is in the **Infrastructure Layer** — it orchestrates downloads using `gglib-hf` for file resolution.

```text
gglib-core (types)          gglib-download            External
┌──────────────────┐        ┌──────────────────┐        ┌──────────────────┐
│  QueueSnapshot   │◄───────│  DownloadManager │───────►│   HuggingFace    │
│  DownloadRow     │        │  DownloadQueue   │        │       Hub        │
│  DownloadEvent   │        │  FileResolver    │        └──────────────────┘
└──────────────────┘        └───────┬──────────┘                 
                                    │                            
                            ┌───────▼──────────┐        ┌──────────────────┐
                            │    gglib-hf      │        │  hf_xet helper   │
                            │  (HfClientPort)  │        │ (opt-in accel.)  │
                            └──────────────────┘        └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                             gglib-download                                          │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │  manager/   │ ──► │   queue/    │ ──► │  executor/  │                            │
│  │  Public API │     │  Task queue │     │  Download   │                            │
│  │  & facade   │     │  & state    │     │  workers    │                            │
│  └─────────────┘     └─────────────┘     └─────────────┘                            │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │  resolver/  │     │  cli_exec/  │                                                │
│  │ File URL &  │     │ OPTIONAL    │                                                │
│  │ shard logic │     │ hf_xet accel│                                                │
│  └─────────────┘     └─────────────┘                                                │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                              depends on  │
                      ┌───────────────────┴───────────────────┐
                      ▼                                       ▼
          ┌───────────────────┐                   ┌───────────────────┐
          │    gglib-core     │                   │     gglib-hf      │
          │  (download types) │                   │  (HfClientPort)   │
          └───────────────────┘                   └───────────────────┘
```

**Module Descriptions:**
- **`quant_selector.rs`** — Quantization selection logic for model downloads
- **`queue/`** — Download task queue with priority and state management
- **`executor/`** — Async download workers with retry logic
- **`resolver/`** — File URL resolution and shard detection
- **`executor/`** — The download backends: `native.rs` (default, `reqwest`) and
  the dispatch that picks between it and the optional accelerator
- **`cli_exec/`** — The optional `hf_xet` Python subprocess accelerator
- **`manager/`** — High-level download manager facade

## Features

- **Queued Downloads** — Multiple concurrent downloads with priority ordering
- **One Row per Download** — The queue is served as one `QueueSnapshot`, on
  the REST route and the event stream alike: the running download, the waiting
  ones, and how the latest ended. A download is one row however many files it
  has, with its bytes over all of them, its phase (`Finalizing` and
  `Registering` between the last byte hitting disk and the model row being
  written), and its text ready to print. A snapshot is sent when the queue
  changes and four times a second while a file is fetched, each with the next
  `revision`.
- **Speed and ETA** — Computed once, by the download's meter, using
  `gglib_core::download::RateEstimator`, and carried on the row for every
  renderer to display verbatim. The speed is taken from the bytes received
  from the network and the time remaining from the bytes on disk (`meter.rs`):
  the accelerator writes to disk in large steps while the network runs flat,
  and a resumed or already-present file is bytes on disk that nobody
  received. Those took no time to arrive, so the time remaining counts them
  as done and leaves them out of its rate. One meter per *download*
  (`manager/meter.rs`), so the reported speed is continuous from its first
  file to its last. Renderers must
  not derive a rate from successive byte counts; `indicatif`'s built-in
  `{bytes_per_sec}` and `{eta}` are deliberately absent from every template
  here, because using them made the CLI and the GUI report different numbers
  for the same transfer. Both are `Option` on the wire and omitted when
  unknown — an absent rate is not a zero rate.
- **Automatic Model Registration** — Downloads are automatically registered in the database with parsed GGUF metadata
- **Resume Support** — Partial download resumption on failure
- **Shard Handling** — Automatic detection and download of sharded models
- **Projectors** — A download from a repository that has projectors fetches
  one with the model, as one more file of its group after the weights: the
  one whose name carries the download's quantization, else the `F16` one,
  else the first by name (`choose_projector`). At completion the projector
  is handed to the registrar apart from the weights, and the model is linked
  to it when the file's header says it is a projector; a model the library
  already holds with a link keeps that link. A projector is never counted as
  a shard.
- **Native Downloads** — The default path is Rust `reqwest`: a resumable ranged
  GET verified against the object's SHA-256, written to `<dest>.part` and
  renamed into place only once it checks out. Needs nothing installed.
- **Optional Acceleration** — `hf_xet` is used for multi-gigabyte files when its
  environment is already provisioned; it is never provisioned implicitly, and
  its absence or failure falls back to the native path. Either way each file
  has one count (`executor/progress.rs`): bytes on disk for the bar, and bytes
  off the network apart from them.
- **Bounded Drain on Cancel** — `cancel_all()` signals cancel tokens and
  then waits up to 5 s for in-flight jobs to finalize before returning,
  so callers (CLI, Tauri, Axum) don't exit while in-flight transfers (or an
  accelerator subprocess) are still cleaning up.
- **Retry Logic** — Automatic retry with exponential backoff

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_download::{build_download_manager, DownloadManagerDeps, DownloadManagerPort};
use gglib_core::download::Quantization;
use gglib_core::ports::DownloadRequest;

// Build the manager with dependencies
let manager: Arc<dyn DownloadManagerPort> = Arc::new(build_download_manager(deps));

// Queue a download with explicit quantization
let request = DownloadRequest::new("TheBloke/Llama-2-7B-GGUF", Quantization::Q4KM);
let id = manager.queue_download(request).await?;

// Or use queue_smart for automatic quantization selection:
// - Single quant available → auto-picks it
// - Multiple quants → uses default preference (Q5_K_M, Q4_K_M, etc.)
// - Explicit quant → validates it exists
//
// Note: Unsloth Dynamic ("UD-") quants (e.g. "UD-Q6_K") are always distinct,
// separately selectable entries from their plain counterparts ("Q6_K") -- they
// are never picked by the default preference list, so request them explicitly.
let (position, shard_count) = Arc::clone(&manager)
    .queue_smart("user/model".to_string(), Some("Q8_0".to_string()))
    .await?;

// Monitor progress via events or poll status
let snapshot = manager.get_queue_snapshot().await?;
```

## Design Decisions

1. **Async Queue** — Downloads run in background with status polling/events
2. **HF Client Injection** — Uses `gglib-hf` via trait for testability
3. **Native by Default** — Rust HTTP always works; `hf_xet` is an opt-in accelerator layered on top, never a prerequisite
4. **Event-Driven** — Progress updates via `AppEventEmitter` for UI decoupling
5. **Automatic Registration** — `ModelRegistrarPort` injected for seamless database integration on download completion
