# queue

<!-- module-docs:start -->

Download queue state machine.

A pure synchronous state machine for managing download queue state. No I/O is performed here — the orchestrator (`DownloadManager`) handles side effects.

## Design

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              queue/                                                 │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│    Method Calls                DownloadQueue               Return Values           │
│   ┌──────────────┐          ┌───────────────┐          ┌──────────────┐            │
│   │queue_sharded │──────────▶│  Pure state  │──────────▶│   Position   │            │
│   │   dequeue    │          │    machine   │          │  QueuedItem  │            │
│   │   reorder    │          │   (no I/O)   │          │   Snapshot   │            │
│   │    remove    │          │              │          │    Error     │            │
│   └──────────────┘          └───────────────┘          └──────────────┘            │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Position Semantics

- **Position 1** = Currently downloading
- **Position 2+** = Waiting in queue
- **Position 0** = Failed (not in active queue)

## Principles

- Pure synchronous state machine (no async, no I/O)
- Direct method calls with Result/Option returns
- Deterministic: same inputs always produce same outputs

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`shard_group.rs`](shard_group) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-shard_group-coverage.json) |
| [`types.rs`](types) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-queue-types-coverage.json) |
<!-- module-table:end -->

</details>
