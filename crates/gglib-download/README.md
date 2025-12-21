# gglib-download

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-complexity.json)

Download queue and manager for `HuggingFace` model files.

## Architecture

This crate is in the **Infrastructure Layer** — it orchestrates downloads using `gglib-hf` for file resolution.

```text
gglib-core (types)          gglib-download            External
┌──────────────────┐        ┌──────────────────┐        ┌──────────────────┐
│  DownloadTask    │◄───────│  DownloadManager │───────►│   HuggingFace    │
│  DownloadStatus  │        │  DownloadQueue   │        │       Hub        │
│  ProgressInfo    │        │  FileResolver    │        └──────────────────┘
└──────────────────┘        └───────┬──────────┘                 
                                    │                            
                            ┌───────▼──────────┐        ┌──────────────────┐
                            │    gglib-hf      │        │   hf_xet helper  │
                            │  (HfClientPort)  │        │  (fast-path DL)  │
                            └──────────────────┘        └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                             gglib-download                                          │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │  manager.rs │ ──► │   queue/    │ ──► │  executor/  │ ──► │  progress/  │        │
│  │  Public API │     │  Task queue │     │  Download   │     │  Tracking   │        │
│  │  & facade   │     │  & state    │     │  workers    │     │  & events   │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │  resolver/  │     │  cli_exec/  │                                                │
│  │ File URL &  │     │  hf_xet CLI │                                                │
│  │ shard logic │     │  subprocess │                                                │
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

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`quant_selector.rs`](src/quant_selector) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-quant_selector-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-quant_selector-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-quant_selector-coverage.json) |
| [`cli_exec/`](src/cli_exec/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-coverage.json) |
| [`executor/`](src/executor/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-coverage.json) |
| [`manager/`](src/manager/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-coverage.json) |
| [`progress/`](src/progress/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-coverage.json) |
| [`queue/`](src/queue/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-coverage.json) |
| [`resolver/`](src/resolver/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-resolver-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-resolver-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-resolver-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`quant_selector.rs`** — Quantization selection logic for model downloads
- **`queue/`** — Download task queue with priority and state management
- **`executor/`** — Async download workers with retry logic
- **`progress/`** — Progress tracking and event emission
- **`resolver/`** — File URL resolution and shard detection
- **`cli_exec/`** — Python `hf_xet` subprocess for fast-path downloads
- **`manager/`** — High-level download manager facade

## Features

- **Queued Downloads** — Multiple concurrent downloads with priority ordering
- **Progress Tracking** — Real-time progress events for UI updates
- **Automatic Model Registration** — Downloads are automatically registered in the database with parsed GGUF metadata
- **Resume Support** — Partial download resumption on failure
- **Shard Handling** — Automatic detection and download of sharded models
- **Fast-Path Downloads** — Uses `hf_xet` Python helper for multi-gigabyte files
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
let (position, shard_count) = Arc::clone(&manager)
    .queue_smart("user/model".to_string(), Some("Q8_0".to_string()))
    .await?;

// Monitor progress via events or poll status
let snapshot = manager.get_queue_snapshot().await?;
```

## Design Decisions

1. **Async Queue** — Downloads run in background with status polling/events
2. **HF Client Injection** — Uses `gglib-hf` via trait for testability
3. **Dual Download Paths** — Native Rust for small files, `hf_xet` for large ones
4. **Event-Driven** — Progress updates via `AppEventEmitter` for UI decoupling
5. **Automatic Registration** — `ModelRegistrarPort` injected for seamless database integration on download completion
