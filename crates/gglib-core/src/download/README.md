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
- `events` - Download events and status types (`DownloadEvent`, `DownloadStatus`).
  `DownloadEvent::DownloadNotice` is the one variant that isn't part of the
  progress/lifecycle state machine: a transient, non-persisted, free-form note
  (e.g. "preparing fast downloader…" while the first-run Python venv builds)
  for renderers to show in place of progress that doesn't exist yet. Unlike
  `DownloadStatusChanged`, it carries arbitrary text rather than a fixed
  `DownloadStatus`, and the next progress or status event overwrites it.
- `errors` - Error types for download operations
- `queue` - Queue snapshot DTOs (`QueueSnapshot`, `QueuedDownload`, `FailedDownload`)
- `completion` - Queue run completion tracking types
- `rate` - `RateEstimator`, the single owner of download speed and ETA math.
  Decays bytes and elapsed time separately so `hf-xet`'s bursty on-disk writes
  do not spike the reported rate. Renderers display what it produces and must
  never re-derive a rate from byte deltas.
- `throttle` - `ProgressThrottle`, the emission rate limiter that runs with
  the estimator. Feed `RateEstimator` every tick; throttle only what you send.
  Two callers: the native download executor in `gglib-download`, and the
  llama.cpp pre-built install pipeline in `gglib-runtime`, which rate-limits
  its `LlamaProgressEvent` channel the same way.
- `format` - `format_rate` / `format_duration`. Rates are **decimal**
  (`1 MB/s` = 1,000,000 B/s) to match what a system network monitor reports;
  sizes stay binary and are rendered by `indicatif`'s `HumanBytes`. Mirrored
  exactly by `formatRate` / `formatDuration` in `src/utils/format.ts`.

<!-- module-docs:end -->
