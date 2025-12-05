# Download Module

<!-- module-docs:start -->

The `download` module provides a clean, opinionated API for downloading models from HuggingFace Hub.
It uses a Python-based executor (via `hf_xet`) for fast, resumable downloads.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                    DownloadManager                              │
│  (thin orchestrator: queue, executor, progress emission)        │
└─────────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌────────────────┐  ┌──────────────────┐  ┌───────────────────┐
│  DownloadQueue │  │ PythonExecutor   │  │ EventCallback     │
│  (pure state)  │  │ (I/O boundary)   │  │ (broadcast/emit)  │
└────────────────┘  └──────────────────┘  └───────────────────┘
         │                    │
         ▼                    ▼
┌────────────────┐  ┌──────────────────┐
│ domain/types   │  │ huggingface/     │
│ domain/events  │  │ (file resolver)  │
│ domain/errors  │  │                  │
└────────────────┘  └──────────────────┘
```

## Domain Types

### Core Identifiers

- **`DownloadId`** - Canonical identifier for a download (`model_id:quantization` or just `model_id`)
- **`ShardGroupId`** - Links shards of the same model together for group operations

### Request & Response

- **`DownloadRequest`** - Parameters for starting a download (repo, files, output directory)
- **`QueueSnapshot`** - Current state of the download queue (pending, current, failed items)
- **`DownloadSummary`** - Lightweight view of a queued item for API responses

### Events

- **`DownloadEvent`** - Discriminated union of all download state changes:
  - `DownloadStarted` - Download began
  - `DownloadProgress` - Progress update (bytes downloaded, speed, ETA)
  - `ShardProgress` - Progress for sharded models (includes aggregate totals)
  - `DownloadCompleted` - Download finished successfully
  - `DownloadFailed` - Download failed with error
  - `DownloadCancelled` - Download was cancelled
  - `QueueSnapshot` - Queue state changed

### Errors

- **`DownloadError`** - Typed errors for download operations:
  - `AlreadyQueued` - Item is already in the queue
  - `NotInQueue` - Item not found in queue
  - `QueueFull` - Queue has reached max capacity
  - `NotFound` - Model/file not found on HuggingFace
  - `Cancelled` - Download was cancelled
  - `ExecutionFailed` - Python executor failed

## Usage

### Via AppCore (Recommended)

The `DownloadManager` is accessed through `AppCore::downloads()`:

```rust,ignore
use gglib::services::core::AppCore;

let core = AppCore::new(pool);

// Queue a download (auto-detects shards)
let (id, position, shard_count) = core.downloads()
    .queue_download_auto("TheBloke/Llama-2-7B-GGUF", "Q4_K_M")
    .await?;

// Get queue state
let snapshot = core.downloads().get_queue_snapshot().await;
println!("Pending: {} items", snapshot.pending.len());

// Cancel a download
core.downloads().cancel(&id).await;
```

### Direct Usage

For testing or standalone use:

```rust,ignore
use gglib::download::{DownloadManager, DownloadManagerConfig, DownloadId};
use std::sync::Arc;

let config = DownloadManagerConfig {
    models_dir: PathBuf::from("./models"),
    max_queue_size: 100,
    hf_token: None,
};

// Create with event callback
let manager = DownloadManager::new(config, Arc::new(|event| {
    println!("Event: {:?}", event);
}));

// Queue and process
let (id, position) = manager.queue(DownloadId::new("org/model", Some("Q4_K_M"))).await?;
manager.process_queue().await?;
```

## Submodules

### `domain/`

Pure domain types with no I/O:
- `types.rs` - `DownloadId`, `DownloadRequest`, `ShardInfo`, `Quantization`
- `events.rs` - `DownloadEvent`, `DownloadStatus`, `DownloadSummary`
- `errors.rs` - `DownloadError` enum

### `queue/`

Sync queue data structure:
- `DownloadQueue` - Manages pending/failed items (wrapped in `RwLock` by manager)
- `QueuedDownload` - Internal queue item with metadata
- `FailedDownload` - Failed item with error message
- `QueueSnapshot` - Serializable queue state for API

### `executor/`

Python-based download execution:
- `PythonDownloadExecutor` - Spawns `hf_xet_downloader.py` subprocess
- `ExecutionResult` - Success/failure with output path
- `EventCallback` - Type alias for progress callbacks

### `huggingface/`

HuggingFace API integration:
- `resolve_quantization_files()` - Find GGUF files for a quantization
- `FileResolution` - Resolved files with sizes and shard detection

### `progress/`

Progress tracking utilities:
- `ProgressThrottle` - Rate-limit progress events
- `ProgressContext` - Track speed and ETA
- `build_queue_snapshot()` - Create `QueueSnapshot` event

## Event Flow

```text
queue_download_auto()
    │
    ├─► resolve_quantization_files() ─► HuggingFace API
    │
    ├─► queue.queue() or queue.queue_sharded()
    │
    └─► emit QueueSnapshot event

process_queue()
    │
    ├─► queue.pop_next()
    │
    ├─► emit DownloadStarted
    │
    ├─► executor.execute()
    │       │
    │       └─► emit DownloadProgress / ShardProgress
    │
    ├─► emit DownloadCompleted / DownloadFailed
    │
    └─► emit QueueSnapshot (updated)
```

## Integration with GUI

The `DownloadManager` supports hot-swappable event callbacks via `set_event_callback()`. GUI backends wire this up during initialization:

- **Tauri (Desktop)**: Events are broadcast via `app_handle.emit("download-progress", ...)`
- **Web (Axum)**: Events are broadcast via SSE at `/api/models/download/progress`

Both backends also call `core.handle_download_completed(id)` to register completed downloads in the database.

### Frontend consumption (React)

Use the unified hook in `src/download/hooks/useDownloadManager`:

```tsx
import { useDownloadManager } from '../download/hooks/useDownloadManager';

const DownloadStatus = () => {
    const { currentProgress, queueStatus, queueModel, cancel, clearFailed, connectionMode } = useDownloadManager({
        onCompleted: () => console.log('Download done; refresh models if needed'),
    });

    return (
        <div>
            <div>Mode: {connectionMode}</div>
            <div>Active: {currentProgress?.id ?? 'none'}</div>
            <div>Pending: {queueStatus?.pending.length ?? 0}</div>
            <button onClick={() => queueModel('TheBloke/Llama-2-7B-GGUF', 'Q4_K_M')}>Queue</button>
            {currentProgress?.id && <button onClick={() => cancel(currentProgress.id)}>Cancel</button>}
            <button onClick={() => clearFailed()}>Clear failed</button>
        </div>
    );
};
```

### UI components

- `GlobalDownloadStatus` shows the active download + queue badge; it no longer renders a completion banner or cancel-confirm modal (keep the page-level banner/notifications in the parent if desired).
- `DownloadProgressDisplay` uses shared progress components and the new download types.

### Tests

- `tests/ts/hooks/useDownloadManager.test.ts` covers queue snapshot init, progress/completion events (with throttling), enqueue/refresh, and subscription cleanup.

<!-- module-docs:end -->
