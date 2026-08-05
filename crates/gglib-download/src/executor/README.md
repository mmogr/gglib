# Executor

<!-- module-docs:start -->

Download execution: the layer that actually moves bytes, given a plan.

[`DownloadPlan`] names a repository, a revision, a destination directory and the
files to fetch. [`download_files`] picks a backend for it:

| Backend | When | Implementation |
|---------|------|----------------|
| Native Rust HTTP | Default | `native.rs` — `reqwest` streaming |
| `hf_xet` accelerator | Only when its environment is **already** provisioned | [`crate::cli_exec::run_fast_download`] |

The accelerator is never provisioned implicitly. Building a Python environment
on demand put a toolchain in the critical path of a new user's first download,
which is the reason this module exists. [`crate::cli_exec::fast_helper_provisioned`]
is a file-existence check, not a probe — if the environment is absent, the
native path runs and nothing is installed. When the accelerator is present but
fails, [`download_files`] logs it, emits a notice and falls back to the native
path; only a user cancellation propagates as-is.

The native path (`native.rs`) is responsible for:

- **Resumable transfers.** Bytes accumulate in `<dest>.part`; a restart sends
  `Range: bytes=<n>-` and appends. A `200` answer to a ranged request means the
  server ignored the range, so the partial file is discarded and the transfer
  starts over.
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
[`DownloadEvent`](gglib_core::download::DownloadEvent)s — it reports raw
`(downloaded, total)` byte counts to a callback and the caller decides what to
do with them.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`native.rs`](native.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native-coverage.json) |
| [`native_tests.rs`](native_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-executor-native_tests-coverage.json) |
<!-- module-table:end -->

</details>
