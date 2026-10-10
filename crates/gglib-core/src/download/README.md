# download

<!-- module-docs:start -->

Download domain types, events, errors, and traits.

This module contains pure data types and trait definitions for the download
system. No I/O, networking, or runtime dependencies allowed.

# Structure

- `types` - Core identifiers and data structures (`DownloadId`, `Quantization`).
  `Quantization` models Unsloth Dynamic ("UD-") quants (e.g. `UD-Q6_K`) as distinct
  values from their plain counterparts (`Q6_K`), since `HuggingFace` repos frequently
  publish both with the same bit-depth suffix.
- `file_role` - What a GGUF file is for (`GgufFileRole`): a model's weights or a
  multimodal projector, read from its file name (`GgufFileRole::classify`), and which of
  a model's own files are projectors (`GgufFileRole::projectors_among`). `gglib-gguf`
  reports the same type from the file's header.
- `projector_choice` - Which of a repository's projectors a download fetches
  (`choose_projector`): the one of the download's own quantization, else the `F16` one,
  else the first by name.
- `shard_info` - One file's place in its download group (`ShardInfo`), as the queue
  records it. A group is the model's weights followed by the projector fetched with them
  and an image model's companions; shards are numbered among the weights alone, and the
  group's size covers every file.
  It is not served: a client reads the `FilePlace` in a row's text.
- `events` - `DownloadEvent`, five variants. `QueueSnapshot` carries the whole
  queue, the same value the REST route serves. `DownloadCompleted`,
  `DownloadFailed` and `DownloadCancelled` say a download ended, for a notice
  to the user and a refresh of the library, each with its finished entry's
  `text`, and `QueueRunComplete` sums up a run. Progress, phases and notes are not events: they are on the snapshot's
  rows.
- `errors` - Error types for download operations
- `queue` - The queue as it is served (`QueueSnapshot`): the running download,
  the waiting ones, and how the latest ended (`FinishedDownload`,
  `DownloadOutcome`), with a `revision` that orders every snapshot a process
  builds. `FinishedDownload::new` is the one place an ending is put in
  words (its `text`), as `DownloadRowText::of` is for a row.
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
- `format` - `format_rate` / `format_duration` / `format_size`. Rates are **decimal**
  (`1 MB/s` = 1,000,000 B/s) to match what a system network monitor reports;
  sizes are binary with two decimals. A value is rounded first and its unit
  chosen after. `formatRate` / `formatDuration` / `formatSize` in
  `src/utils/format.ts` are the same functions, and `format_vectors.json` is
  the list of inputs and texts the tests of both read.
- `row` - One download as one line (`DownloadRow`, `DownloadPhase`,
  `DownloadRowText`). `row(&RowFacts)` builds it from the facts of a download
  and `DownloadRowText::of` words it, so every surface prints the same text.
  `download_title` is the one name a download goes by, and `FilePlace` the file
  a row is about: `part 2/3`, `weights`, `projector`, a companion's role such as `vae`,
  or `3 parts`.

<!-- module-docs:end -->
