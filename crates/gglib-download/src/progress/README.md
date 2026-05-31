# progress

<!-- module-docs:start -->

Progress tracking, throttling, and speed estimation.

This module handles progress aggregation, rate-limiting, and accurate speed/ETA
calculation for download progress events.

## Components

| Component | Description |
|-----------|-------------|
| `ProgressThrottle` | Rate-limits progress events (e.g., max 10/sec) |
| `SlidingWindowRate` | 8-second sliding window speed estimator (bytes/sec) |
| `format_eta` | Formats an ETA in seconds as `M:SS` or `H:MM:SS` |

## Design

Downloads may report progress thousands of times per second. This module aggregates updates and emits throttled events suitable for UI consumption.

`SlidingWindowRate` replaces the previous α=0.02 exponentially-weighted-average approach. It retains timestamped byte-count samples over an 8-second window and reports `(Δbytes / Δtime)` across that window, which closely matches what system monitors show.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`rate.rs`](rate.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-rate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-rate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-rate-coverage.json) |
| [`throttle.rs`](throttle.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-coverage.json) |
<!-- module-table:end -->

</details>
