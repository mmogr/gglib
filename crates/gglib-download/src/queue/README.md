# queue

<!-- module-docs:start -->

Download queue management.

This module provides a pure state machine for managing download queue state.
No I/O is performed here; the orchestrator (`DownloadManager`) handles I/O.

# Design

- Pure synchronous state machine (no async, no IO, no tracing)
- Commands produce events that the caller can use for side effects
- Deterministic: same inputs always produce same outputs

# Downloads and files

- The queue holds one item per file; every file of a download carries the
  download's id
- Rows, positions and capacity count downloads, never files (`rows.rs`)
- The running download is the one being fetched, or the one between two of
  its files. Its pending files stay at the head of the queue: they are not a
  row, take no place, and nothing is moved in front of them

# Position Semantics

- Position 1 = the running download
- Position 2+ = the waiting downloads, in the order they will run
- With nothing running, the first waiting download is at position 1
- Failed items have position 0 (not in active queue)

<!-- module-docs:end -->
