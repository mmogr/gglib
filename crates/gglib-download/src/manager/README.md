# manager

Download manager coordinating queues, executors, and state tracking.

## Purpose

This module implements the **download manager** that orchestrates the entire download lifecycle:
- Queue management
- Concurrent download coordination
- State tracking and persistence
- Progress reporting
- Error handling and retries
- Cancellation and pause/resume

## Architecture Pattern

**Multi-file Download Orchestration**

```text
┌─────────────────────────────────────────────────────────────┐
│                    Download Manager                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐              │
│  │  Queue   │───▶│ Executor │───▶│  State   │              │
│  │ Manager  │    │  Pool    │    │ Tracking │              │
│  └──────────┘    └──────────┘    └──────────┘              │
│       │               │               │                      │
│       ▼               ▼               ▼                      │
│  ┌──────────────────────────────────────────────┐           │
│  │         Background Task Coordinator          │           │
│  └──────────────────────────────────────────────┘           │
│                       │                                      │
│                       ▼                                      │
│  ┌──────────────────────────────────────────────┐           │
│  │         Progress Event Emission              │           │
│  └──────────────────────────────────────────────┘           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## File Organization

### Main Manager
- **`background.rs`** - Background download coordinator
  - Spawns and manages download tasks
  - Handles concurrent downloads
  - Task lifecycle management
  - Graceful shutdown

- **`multi.rs`** - Multi-file download orchestration
  - Coordinate downloading multiple files for one model
  - Track individual file progress
  - Aggregate progress reporting
  - Handle partial failures

### State Management
- **`state.rs`** - Download state tracking
  - Active downloads registry
  - State persistence
  - State transitions (queued → active → completed/failed)
  - Recovery from crashes

### Error Handling
- **`error.rs`** - Manager-specific error types
  - Queue errors
  - Executor errors
  - State corruption errors
  - Coordination errors

## Coordination Flow

### 1. Enqueue Download
```rust
use gglib_download::manager::background::BackgroundManager;

let manager = BackgroundManager::new(state, executor, queue);
manager.enqueue_download(download_request).await?;
```

### 2. Background Processing
```rust
// Manager spawns background task
tokio::spawn(async move {
    loop {
        let next = queue.pop_next().await?;
        executor.execute(next).await?;
        state.mark_completed(next.id).await?;
    }
});
```

### 3. Progress Reporting
```rust
// Manager emits progress events
manager.on_progress(|progress| {
    event_emitter.emit(DownloadProgress {
        id: progress.id,
        bytes_downloaded: progress.bytes,
        total_bytes: progress.total,
    });
});
```

### 4. Cancellation
```rust
manager.cancel_download(download_id).await?;
// Manager coordinates:
// 1. Stop executor
// 2. Update state
// 3. Clean up partial files
// 4. Emit cancellation event
```

## Concurrency Strategy

### Bounded Parallelism
```rust
const MAX_CONCURRENT: usize = 3;

// Manager limits concurrent downloads
let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
```

### Task Coordination
- One task per active download
- Shared state protected by `RwLock` or `Mutex`
- Channels for progress updates
- Graceful cancellation with `CancellationToken`

## State Transitions

```text
Queued → Active → Completed
  ↓        ↓          ↓
Cancelled  ↓      Failed
           ↓          ↓
        Paused ──→ Resumed
```

## Error Handling Strategy

### Retry Logic
- Transient network errors: retry with exponential backoff
- HTTP 429/503: retry with rate limiting
- HTTP 404/403: fail immediately
- Disk full: pause all downloads

### Partial Failure Handling
For multi-file downloads:
- Continue downloading other files
- Track which files succeeded
- Allow resume of failed files
- Report aggregate status

## Dependencies

- **Queue**: `../queue/` for download queuing
- **Executor**: `../executor/` for actual HTTP downloads
- **Progress**: `../progress/` for progress tracking
- **Core types**: `gglib-core::download::*` for domain types
- **Events**: `gglib-core::events::download` for event emission

## Testing

Manager tests focus on:
- Coordination between components
- State consistency during failures
- Cancellation correctness
- Progress reporting accuracy
- Concurrent download handling

Use mock executors and queues for unit tests.

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`paths.rs`](paths) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-coverage.json) |
| [`shard_group_tracker.rs`](shard_group_tracker) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-coverage.json) |
| [`worker.rs`](worker) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-coverage.json) |
<!-- module-table:end -->
