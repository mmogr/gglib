# CLI Exec

<!-- module-docs:start -->

CLI download utility layer.

This module provides utilities used by CLI commands that are intentionally
separated from the queue-based [`DownloadManagerPort`] path.

# What lives here

- [`list_quantizations`] — `HuggingFace` quant listing for `--list-quants`
- [`check_update`] / [`update_model`] — update path for `model upgrade`
- Python bridge helpers ([`ensure_fast_helper_ready`], [`run_fast_download`]) shared
  with the async download manager

# What moved out

Interactive downloads (the `model download` command) now route through
[`DownloadManagerPort::queue_smart`], giving the CLI the same queue,
progress events, and model registration path as the GUI.

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
