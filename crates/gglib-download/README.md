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
│  │ Files and   │     │ OPTIONAL    │                                                │
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
- **`queue/`** — The queue's state: what waits, in what order, and how the
  latest downloads ended
- **`resolver/`** — Which files a quantization is, and whether its weights
  are sharded
- **`executor/`** — The download backends: `native.rs` (default, `reqwest`) and
  the dispatch that picks between it and the optional accelerator
- **`cli_exec/`** — The optional `hf_xet` Python subprocess accelerator, and
  what `gglib model` runs off the queue: the quantization listing, the update
  check and the upgrade's download
- **`manager/`** — High-level download manager facade
- **`solo.rs`** — One download fetched without the queue (`model upgrade`),
  as the row the queue would show for it

## Features

- **Queued Downloads** — One download runs at a time and the rest wait in the
  order they were queued. A waiting one can be moved or taken off. There is
  one way to queue (`queue_smart`), and it starts the runner, so a download
  queued by a repair is fetched like one a user asked for.
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
- **One Row Off the Queue Too** — `gglib model upgrade` fetches its files
  without the queue (`solo.rs`), and is one row all the same. The row is put
  together where the queue's running row is (`queue::running_row`), from
  files placed by the function that places a queued group's and a meter of
  the kind a queued download has, so its bytes carry on from one file to
  the next. The row is handed to the caller's `RowCallback` four times a
  second while a file is fetched and as each file lands. This crate draws
  nothing on a terminal and does not depend on `indicatif`.
- **Automatic Model Registration** — Downloads are automatically registered in the database with parsed GGUF metadata.
  A weights file the GGUF reader refuses is registered all the same, without that
  metadata, and the download's completion message names the file and gives the
  reader's reason
- **Resume Support** — A native transfer cut off part-way leaves its `.part`
  file, and the next download of that file asks only for the bytes that are
  missing. A failed download is not retried: it ends as failed, and is
  fetched again when it is queued again.
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
- **The Models Directory, Asked as a Download Starts** — A download goes
  under the models directory `gglib_core::paths::resolve_models_dir` answers
  as its first file starts (`manager/paths.rs`), and every file of it goes
  there. Unless its config names a directory, the manager keeps none from
  when it was built, so a daemon's next download goes where the directory
  resolves then: one stored while the daemon runs, unless the daemon's own
  environment names one.
- **Whole-Download Endings** — A download ends as a whole
  (`manager/ending.rs`). A file that fails, a cancel or a removal takes every
  file of the download off the queue, and the download leaves one outcome:
  completed, failed or cancelled. A cancel wins over a file that lands all
  the same: the model is not registered.
- **Bounded Drain on Cancel** — `cancel_all()` ends each waiting download as
  cancelled, tells the transfer in flight to stop, and
  then waits up to 5 s for in-flight jobs to finalize before returning,
  so callers (CLI, Tauri, Axum) don't exit while in-flight transfers (or an
  accelerator subprocess) are still cleaning up.

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_download::{build_download_manager, DownloadManagerDeps, DownloadManagerPort};

// Build the manager with dependencies
let manager: Arc<dyn DownloadManagerPort> = Arc::new(build_download_manager(deps));

// Queue a download. It answers the download's ID, and starts the runner.
// - Explicit quant → validates it exists
// - No quant, single one available → auto-picks it
// - No quant, several → uses default preference (Q5_K_M, Q4_K_M, etc.)
//
// Note: Unsloth Dynamic ("UD-") quants (e.g. "UD-Q6_K") are always distinct,
// separately selectable entries from their plain counterparts ("Q6_K") -- they
// are never picked by the default preference list, so request them explicitly.
let id = Arc::clone(&manager)
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
