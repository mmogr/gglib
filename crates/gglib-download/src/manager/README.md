# manager

<!-- module-docs:start -->

Download manager implementation.

This module provides the concrete implementation of `DownloadManagerPort`
with a long-lived runner, lease-based state management, and clean separation
between the worker (core download logic) and bridges (event emission).

# Architecture

- **Manager**: Orchestrates queue, leases, and worker lifecycle
- **Worker**: Executes downloads, writes only to `watch::Sender` (no events)
- **Bridge tasks**: Subscribe to watch channels, emit events with rate-limiting

# Concurrency Model

- Single long-lived runner (never resets `runner_started`)
- `Notify` for efficient wake-on-work
- Lease tokens prevent stale finalize commits
- Lock order: queue → active (consistent everywhere)

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`paths.rs`](paths.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-paths-coverage.json) |
| [`shard_group_tracker.rs`](shard_group_tracker.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-shard_group_tracker-coverage.json) |
| [`worker.rs`](worker.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-manager-worker-coverage.json) |
<!-- module-table:end -->

</details>
