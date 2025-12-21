# exec

<!-- module-docs:start -->

Download execution implementation.

Core download logic including:

- `download()` — Main download function with progress tracking
- `update_model()` — Check for and apply model updates
- `check_update()` — Check if a model has updates available
- `python_bridge` — Interface to `hf_xet` fast download helper

## Python Bridge

For large downloads, uses `hf_xet` CLI tool (a Rust-based fast downloader) via subprocess for optimal throughput.

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
<!-- module-table:end -->

</details>
