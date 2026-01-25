# paths

<!-- module-docs:start -->

Path utilities for gglib data directories and user-configurable locations.

This module provides canonical path resolution for all gglib components across platforms (macOS, Linux, Windows).

## Key Functions

| Function | Description |
|----------|-------------|
| `data_root()` | Platform-specific data directory (`~/.local/share/gglib` on Linux) |
| `resource_root()` | Bundled resources directory (app bundle on macOS) |
| `database_path()` | SQLite database location |
| `llama_server_path()` | Path to `llama-server` binary |
| `default_models_dir()` | Default directory for downloaded models |

## Platform Defaults

```text
macOS:   ~/Library/Application Support/gglib/
Linux:   ~/.local/share/gglib/
Windows: %APPDATA%\gglib\
```

## Design

- Returns `PathBuf` with clear `PathError` for missing files
- No interactive/terminal I/O — adapters handle user prompts
- Platform logic is private in `platform` submodule

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-config-coverage.json) |
| [`database.rs`](database.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-database-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-database-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-database-coverage.json) |
| [`ensure.rs`](ensure.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-ensure-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-ensure-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-ensure-coverage.json) |
| [`error.rs`](error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-error-coverage.json) |
| [`llama.rs`](llama.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-llama-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-llama-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-llama-coverage.json) |
| [`models.rs`](models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-models-coverage.json) |
| [`pids.rs`](pids.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-pids-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-pids-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-pids-coverage.json) |
| [`platform.rs`](platform.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-platform-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-platform-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-platform-coverage.json) |
| [`resolver.rs`](resolver.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-resolver-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-resolver-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-resolver-coverage.json) |
| [`test_utils.rs`](test_utils.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-test_utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-test_utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-test_utils-coverage.json) |
<!-- module-table:end -->

</details>
