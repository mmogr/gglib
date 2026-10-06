# manager

<!-- module-docs:start -->

Download manager implementation.

This module provides the concrete implementation of `DownloadManagerPort`
with a long-lived runner, lease-based state management, and clean separation
between the worker (core download logic) and publishing (the queue snapshot).

# Architecture

- **Manager**: Orchestrates queue, leases, and worker lifecycle
- **Worker**: Executes downloads, writes only to `watch::Sender` (no events):
  the file's progress, and a note standing in for it (e.g. the optional
  `hf_xet` accelerator being unavailable, so the transfer falls back to the
  native path)
- **Meter** (`meter.rs`): One per download, from its first file to its last.
  It carries the bytes of the files already in, so the download's bytes never
  fall back at a file boundary, and feeds the speed the bytes received and the
  time remaining the bytes on disk. A task samples the worker's channel into
  it four times a second and publishes. A download queued again starts with
  no meter, whatever an earlier run of it left
- **Publishing** (`publish.rs`): One builder makes the queue snapshot for the
  REST route and the event stream alike. Every snapshot takes the next
  `revision` under the publish mutex, which is held until the snapshot is
  sent, so they leave in order. A snapshot is published when the queue
  changes, on every meter tick, and at each change of phase: `Downloading`,
  then `Finalizing` and `Registering` once the last file is in
- **Outcomes**: When a download ends, its outcome joins the queue's finished
  list under the same guard that takes its file out of `active`, and one event
  says so. A model the library refuses is a failed download
- **Group**: A model is queued as one group of files (`enqueue.rs`): its weights,
  then the projector fetched with them. The group is registered once every file
  is on disk (`group_completion.rs`), with the weights as the model's files and
  the projector handed to the registrar apart, to be linked. A projector's bytes
  are the model's progress, and its row reads `projector`, never a shard.
- **Running download**: The group being fetched, or the one between two of its
  files (`running.rs`). The queue snapshot has it as `active`, at position 1,
  across its file boundaries, read from its meter; every other group is one
  waiting row. A group is queued once: a repeat request attaches to it.

# Concurrency Model

- Single long-lived runner (never resets `runner_started`)
- `Notify` for efficient wake-on-work
- Lease tokens prevent stale finalize commits
- Lock order: publish → queue → active → tracker → meters (consistent
  everywhere); the meters are a std mutex, never held across an await
- A file is started under the queue lock, so it is never off the queue and
  not yet active

<!-- module-docs:end -->
