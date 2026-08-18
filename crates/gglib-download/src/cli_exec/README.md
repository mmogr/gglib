# CLI Exec

<!-- module-docs:start -->

CLI download utility layer.

This module provides utilities used by CLI commands that are intentionally
separated from the queue-based
[`DownloadManagerPort`](gglib_core::ports::DownloadManagerPort) path.

# What lives here

- [`list_quantizations`] — `HuggingFace` quant listing for `--list-quants`
- [`check_update`] / [`update_model`] — update path for `model upgrade`
- The optional `hf_xet` accelerator: [`ensure_fast_helper_ready`] provisions it
  (only from an explicit opt-in — never from a download),
  [`fast_helper_provisioned`] reports whether it is already here, and
  [`run_fast_download`] drives it. `crate::executor` owns the choice of when to
  use it; the default download path is native Rust and needs none of this.
  [`fast_helper_status`] describes what is on disk and [`remove_fast_helper`]
  deletes it, both for `gglib config fast-downloads` — neither is on the
  download path, which still turns on the bare file check.

# What moved out

Interactive downloads (the `model download` command) now route through
[`DownloadManagerPort::queue_smart`](gglib_core::ports::DownloadManagerPort::queue_smart),
giving the CLI the same queue,
progress events, and model registration path as the GUI.

# Console output

Every `println!`-shaped line this layer produces (venv setup notes, the
`[fast-path]` passthrough for non-protocol Python output, `model upgrade`'s
status lines) goes through `gglib_core::telemetry::console_println` instead
of a direct `println!`/`eprintln!`. With no hook installed it's a plain
`eprintln!`; the queued-download path installs a hook
(`CliDownloadEventEmitter`) that routes it through the live
`MultiProgress::println` so it can't corrupt a bar's redraw bookkeeping. See
[`gglib_core::telemetry`](../../../gglib-core/src/telemetry.rs) and the
[`exec/`](exec/) submodule below for `FastDownloadRequest::notice`, which
does the same thing for a specific download's bar instead of the shared
console.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`api.rs`](api.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-coverage.json) |
| [`utils.rs`](utils.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-coverage.json) |
| [`exec/`](exec/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-coverage.json) |
<!-- module-table:end -->

</details>
