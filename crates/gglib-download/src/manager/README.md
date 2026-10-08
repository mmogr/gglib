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
  it four times a second and publishes. The meter is dropped when its
  download ends, so a download queued again counts from nothing
- **Publishing** (`publish.rs`): One builder makes the queue snapshot for the
  REST route and the event stream alike. Every snapshot takes the next
  `revision` under the publish mutex, which is held until the snapshot is
  sent, so they leave in order. A snapshot is published when the queue
  changes, on every meter tick, and at each change of phase: `Downloading`,
  then `Finalizing` and `Registering` once the last file is in
- **Destination** (`paths.rs`): A download goes under the models directory
  current as its first file starts: the one `resolve_models_dir` answers then,
  unless the manager's config names one. The files after it go under the same
  one, and it is dropped when the download ends. So the next download goes
  where the directory resolves then, which follows one stored while the
  manager runs, from the settings page or by `gglib config models-dir set`,
  and a download part fetched is not split across two
- **Ending** (`ending.rs`): A download ends as a whole, in one place,
  `end_download`: its files still pending leave the queue, its group is closed
  in the tracker, its meter and its models directory are dropped, and its
  outcome joins the queue's finished list and the run's summary, all under
  one queue guard, the one that takes its file out of `active`. One event
  then says so. It ends when its last file is registered, when a file of it
  fails, or when the user stops it. A model the library refuses is a failed
  download
- **Stopping**: A download waiting, or between two of its files, ends at once
  as cancelled. One with a file being fetched has its worker told to stop and
  ends when the worker returns: cancelled whatever the worker answered, and
  never registered. Cancelling and removing both do this; removing a download
  that has already ended drops its finished entry. Cancelling everything ends
  each download with its own outcome. A cancel can come too late: once the
  download's last file has landed and its model is being registered, or once
  a file of it has failed, it ends completed or failed as it was going to,
  and the cancel is still accepted
- **Tracker** (`shard_group_tracker.rs`): Counts a group's files in. A group
  that was closed stays closed to a file landing late
- **Group**: A model is queued as one group of files (`enqueue.rs`): its weights,
  then the projector fetched with them. The group is registered once every file
  is on disk (`group_completion.rs`), with the weights as the model's files and
  the projector handed to the registrar apart, to be linked. A projector's bytes
  are the model's progress, and its row reads `projector`, never a shard. The
  completion message says what the registrar answered: a projector it did not
  link, and weights the GGUF reader refused, which are in the library without
  their details.
- **Running download**: The group being fetched, or the one between two of its
  files (`running.rs`). The queue snapshot has it as `active`, at position 1,
  across its file boundaries, read from its meter; every other group is one
  waiting row. A group is queued once: a repeat request attaches to it.

# Concurrency Model

- Single long-lived runner (never resets `runner_started`)
- `Notify` for efficient wake-on-work
- Lease tokens prevent stale finalize commits
- Lock order: publish → queue → active → tracker → meters (consistent
  everywhere); the meters are a std mutex, never held across an await, and so
  are the started downloads' directories, taken with no other std mutex held
- A file is started under the queue lock, so it is never off the queue and
  not yet active

<!-- module-docs:end -->
