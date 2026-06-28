# queue

<!-- module-docs:start -->

Download queue management.

This module provides a pure state machine for managing download queue state.
No I/O is performed here; the orchestrator (`DownloadManager`) handles I/O.

# Design

- Pure synchronous state machine (no async, no IO, no tracing)
- Commands produce events that the caller can use for side effects
- Deterministic: same inputs always produce same outputs

# Position Semantics

- Position 1 = currently downloading
- Position 2+ = waiting in queue
- Failed items have position 0 (not in active queue)

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`shard_group.rs`](shard_group.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-coverage.json) |
<!-- module-table:end -->

</details>
