# Executor

<!-- module-docs:start -->

Download execution: the layer that actually moves bytes, given a plan.

[`DownloadPlan`] names a repository, a revision, a destination directory and
one file to fetch. [`download_file`] takes it with the address the file's
bytes are read from, which the Hub the download was resolved on gives
(`HfClientPort::file_url`), and picks a backend for it:

| Backend | When | Implementation |
|---------|------|----------------|
| Native Rust HTTP | Default | `native.rs` — `reqwest` streaming |
| `hf_xet` accelerator | Only when its environment is **already** provisioned | [`crate::cli_exec::run_fast_download`] |

The accelerator is never provisioned implicitly. Building a Python environment
on demand put a toolchain in the critical path of a new user's first download,
which is the reason this module exists. [`crate::cli_exec::fast_helper_provisioned`]
is a file-existence check, not a probe — if the environment is absent, the
native path runs and nothing is installed. When the accelerator is present but
fails, [`download_file`] logs it, emits a notice and falls back to the native
path; only a user cancellation propagates as-is.

Whichever backend runs, the caller is shown one count for the file. A backend
reports readings (`RawProgress`): bytes of the file on disk, and bytes received
from the network, which are different numbers. On a resume the first is ahead
by what was already there, and the accelerator receives well ahead of what it
has written. `FileCounter` (`progress.rs`) turns the readings into the file's
[`FileProgress`]:

- `bytes` follows the bytes on disk and never falls. It stops one byte short
  of the size until [`download_file`] has the finished file in place, so a
  bar reaches 100% only for a file that is there. A file already on disk goes
  straight to it.
- `wire` is the network bytes, summed over a fallback, and never falls. Bytes
  found on disk are not in it, which is what makes it the number to take a
  speed from.
- `size` is the size from metadata, or failing that the first one a backend
  reports. A size of 0 is treated as not known.

When the accelerator fails part-way, the native path starts the file again
from its own partial file, so `bytes` may fall once, at the point the fallback
notice is sent.

The native path (`native.rs`) is responsible for:

- **Resumable transfers.** Bytes accumulate in `<dest>.part`, or for a fetch
  over a copy on disk (an upgrade) in `<dest>.<commit>.part`, every other part
  of that file being removed first; a restart sends `Range: bytes=<n>-` and
  appends. A `200` answer to a ranged request means the server ignored the
  range, so the partial file is discarded and the transfer starts over. A
  `206` is placed only when its `Content-Range` says it starts at the byte
  asked for, and, when it gives the file's size and the size is known, that
  size; otherwise it is refused. The bytes already on disk are read through
  the hasher before the connection opens, so no answer sits unread while
  they are, and on a blocking thread, never an async one (a 3.85 GiB part
  read on the runtime once left `/health` unanswered for 35 s), with a note
  on the row while it runs; a cancel stops the read between two 1 MiB blocks.
- **A connection that goes quiet.** Each wait on the socket, for headers or
  for the next chunk, is bounded by `STALL_AFTER` (30 s; a parameter of the
  transfer, so tests run it in a second). A quiet connection is dropped and
  the transfer carries on from the bytes on disk over a new one, keeping its
  hash and its count of bytes received, up to `STALL_RECONNECTS` (3) times in
  a row with nothing arriving; a connection that takes the part past the
  furthest it has reached starts the count again. Once a download has had
  bytes or a quiet connection, a connection that breaks (a reset) or does
  not open (`CONNECT_TIMEOUT`, 20 s) is lost the same way and counts the
  same: a reset that brought bytes is reconnected at once, and one that
  brought nothing is tried again a quiet time after it began, so a network
  that is down has the reconnects' whole span, about 2 minutes, to come back
  in. Then it fails with `DownloadError::Interrupted`, whose sentence says
  for how long nothing arrived and that trying again carries on from the
  part. The first connection of a download that has had nothing ends the
  download when it fails, spending no reconnects, and its error names the
  cause (`could not connect to <host>: …`).
- **One file, not two spliced.** The first connection's digest and size are
  kept, and a later connection that gives another is of a changed file: the
  part is removed and the file started over, once, with a note on the row.
  Across two runs only the final check below protects, and only with a
  digest.
- **Checksum verification.** SHA-256 is computed while streaming and compared
  against `X-Linked-Etag`, the *only* header carrying the file's true digest.
  A mismatch deletes the `.part` file — resuming onto known-bad bytes would fail
  identically forever. When no `X-Linked-Etag` is offered, the size check carries
  verification instead.
- **Redirects followed by hand.** `HuggingFace`'s `resolve/` endpoint answers
  with a 302 that carries `X-Linked-Etag` and points at a CDN. Letting `reqwest`
  follow that automatically discards the hop the digest lives on, leaving only
  the CDN's own `ETag` — which is the Xet *block* hash echoed from the URL path.
  That value is also 64 hex characters, so no shape check can distinguish it from
  a content digest, and comparing a file against it fails **every** download.
  Hence two rules that must stay together: the client is built with
  `redirect::Policy::none()` (see `native::build_client`, which the tests share
  so the two cannot drift), and plain `ETag` is never accepted as a digest.
- **Atomic publication.** The final path is only ever created by renaming a
  fully verified `.part` file, so a partial transfer can never be mistaken for a
  complete model.

It is **not** responsible for resolving quantizations to files (`resolver`),
queueing (`queue`), or emitting
[`DownloadEvent`](gglib_core::download::DownloadEvent)s — it reports its
readings to a callback and the caller decides what to do with them.

<!-- module-docs:end -->
