# exec

<!-- module-docs:start -->

The optional `hf_xet` download accelerator: the Python environment that backs
it, and the subprocess bridge that drives it. Kept separate from queue
management, and from the default download path — that is `crate::executor`,
which is native Rust and reaches this module only when
[`python_env::fast_helper_provisioned`] says the environment is already here.

**Nothing in this module runs implicitly.** `python_env.rs` builds the venv, but
`PythonEnvironment::prepare` is reachable only from `ensure_fast_helper_ready`,
which in turn is only called by an explicit opt-in:
`gglib config check-deps --setup-fast-downloads` or the GUI setup wizard. A
download never provisions anything; if the environment is absent, the native
path runs instead. Putting a Python toolchain in the critical path of a first
download is the failure mode this arrangement exists to prevent.

`PythonEnvironment::prepare` takes an optional `NoticeCallback`
(`Option<&NoticeCallback>`, aliased in `python_bridge.rs`): with one supplied,
venv creation and dependency install surface as a
`DownloadEvent::DownloadNotice` on a progress bar instead of a console line;
without one (preflight, `model upgrade`) they fall back to
`gglib_core::telemetry::console_println`. `python -m venv` runs via `.output()`,
not `.status()`, so its own stdio is captured rather than inherited — an
inherited handle would write straight to the terminal, outside any bar's
bookkeeping, the same way a stray `println!` would.

`progress.rs`'s `CliProgressPrinter` (the no-callback path, e.g. `model
upgrade`) draws to **stderr**, matching `CliDownloadEventEmitter`'s
`MultiProgress` (indicatif's stderr default) — see the doc comment on
`CliProgressPrinter::new`.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`progress.rs`](progress.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-progress-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-progress-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-progress-coverage.json) |
| [`python_bridge.rs`](python_bridge.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_bridge-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_bridge-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_bridge-coverage.json) |
| [`python_env.rs`](python_env.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_env-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_env-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_env-coverage.json) |
| [`python_protocol.rs`](python_protocol.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_protocol-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_protocol-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-python_protocol-coverage.json) |
| [`xet_poller.rs`](xet_poller.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-xet_poller-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-xet_poller-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-xet_poller-coverage.json) |
<!-- module-table:end -->

</details>
