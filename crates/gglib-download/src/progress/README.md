# progress

<!-- module-docs:start -->

Progress tracking and throttling.

This module handles progress aggregation and rate-limiting for download progress events to prevent UI flooding.

## Components

| Component | Description |
|-----------|-------------|
| `ProgressThrottle` | Rate-limits progress events (e.g., max 10/sec) |

## Design

Downloads may report progress thousands of times per second. This module aggregates updates and emits throttled events suitable for UI consumption.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`throttle.rs`](throttle.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-progress-throttle-coverage.json) |
<!-- module-table:end -->

</details>
