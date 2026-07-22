# download

<!-- module-docs:start -->

Download domain types, events, errors, and traits.

This module contains pure data types and trait definitions for the download
system. No I/O, networking, or runtime dependencies allowed.

# Structure

- `types` - Core identifiers and data structures (`DownloadId`, `Quantization`, `ShardInfo`).
  `Quantization` models Unsloth Dynamic ("UD-") quants (e.g. `UD-Q6_K`) as distinct
  values from their plain counterparts (`Q6_K`), since `HuggingFace` repos frequently
  publish both with the same bit-depth suffix.
- `events` - Download events and status types (`DownloadEvent`, `DownloadStatus`)
- `errors` - Error types for download operations
- `queue` - Queue snapshot DTOs (`QueueSnapshot`, `QueuedDownload`, `FailedDownload`)
- `completion` - Queue run completion tracking types
- `rate` - `RateEstimator`, the single owner of download speed and ETA math.
  Decays bytes and elapsed time separately so `hf-xet`'s bursty on-disk writes
  do not spike the reported rate. Renderers display what it produces and must
  never re-derive a rate from byte deltas.
- `format` - `format_rate` / `format_duration`. Rates are **decimal**
  (`1 MB/s` = 1,000,000 B/s) to match what a system network monitor reports;
  sizes stay binary and are rendered by `indicatif`'s `HumanBytes`. Mirrored
  exactly by `formatRate` / `formatDuration` in `src/utils/format.ts`.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`completion.rs`](completion.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-completion-coverage.json) |
| [`errors.rs`](errors.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-errors-coverage.json) |
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-events-coverage.json) |
| [`format.rs`](format.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-format-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-format-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-format-coverage.json) |
| [`queue.rs`](queue.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-queue-coverage.json) |
| [`rate.rs`](rate.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-rate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-rate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-rate-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-types-coverage.json) |
<!-- module-table:end -->

</details>
