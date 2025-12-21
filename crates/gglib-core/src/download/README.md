# download

<!-- module-docs:start -->

Download domain types, events, and error definitions.

This module contains pure data types and traits for the download system. No I/O, networking, or runtime dependencies — those live in `gglib-download`.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              download/                                              │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   types     │  │   events    │  │   errors    │  │   queue     │                 │
│  │ DownloadId  │  │DownloadEvent│  │DownloadError│  │QueueSnapshot│                 │
│  │ Quantization│  │DownloadStatus│ │DownloadResult│ │QueuedDownload│                │
│  │  ShardInfo  │  │             │  │             │  │FailedDownload│                │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Types

| Type | Description |
|------|-------------|
| `DownloadId` | Unique identifier for a download job |
| `Quantization` | Quantization level (Q4_0, Q8_0, etc.) |
| `DownloadEvent` | Progress/completion events for UI updates |
| `QueueSnapshot` | Point-in-time view of the download queue |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`completion.rs`](completion) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-coverage.json) |
| [`errors.rs`](errors) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-coverage.json) |
| [`events.rs`](events) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-coverage.json) |
| [`queue.rs`](queue) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-coverage.json) |
| [`types.rs`](types) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-coverage.json) |
<!-- module-table:end -->

</details>
