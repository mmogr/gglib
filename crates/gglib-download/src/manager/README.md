# manager

<!-- module-docs:start -->

Download manager implementation.

This module provides the concrete implementation of `DownloadManagerPort`
with a long-lived runner, lease-based state management, and clean separation
between the worker (core download logic) and bridges (event emission).

# Architecture

- **Manager**: Orchestrates queue, leases, and worker lifecycle
- **Worker**: Executes downloads, writes only to `watch::Sender` (no events) —
  with one narrow, deliberate exception: `WorkerDeps.event_emitter` lets
  `execute_download` emit `DownloadEvent::DownloadNotice` directly for
  transient, cosmetic notes (e.g. the optional `hf_xet` accelerator being
  unavailable, so the transfer falls back to the native path) that aren't part
  of the progress or completion state the manager sequences. See the doc
  comment on `WorkerDeps` in `worker.rs`.
- **Bridge tasks**: Subscribe to watch channels, emit events with rate-limiting

# Concurrency Model

- Single long-lived runner (never resets `runner_started`)
- `Notify` for efficient wake-on-work
- Lease tokens prevent stale finalize commits
- Lock order: queue → active (consistent everywhere)

<!-- module-docs:end -->
