# cli_exec

<!-- module-docs:start -->

CLI download execution layer.

Provides download execution logic for CLI commands, intentionally separated from the async queue-based `DownloadManagerPort` designed for GUI/background downloads.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              cli_exec/                                             │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │     api     │  │    exec     │  │    types    │  │    utils    │                 │
│  │search, list│  │  download   │  │  requests   │  │   helpers   │                 │
│  │quantizations│  │   update    │  │  results    │  │             │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Functions

| Function | Description |
|----------|-------------|
| `download()` | Download a model with terminal progress bar |
| `update_model()` | Check and apply updates to a model |
| `search_models()` | Search HuggingFace for models |
| `list_quantizations()` | List available quantizations for a repo |

## Design Principles

- Synchronous/blocking patterns suitable for CLI UX
- Progress bars displayed directly in terminal
- **No `AppCore`** — database registration is the handler's job

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`api.rs`](api) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-api-coverage.json) |
| [`types.rs`](types) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-types-coverage.json) |
| [`utils.rs`](utils) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-cli_exec-utils-coverage.json) |
| [`exec/`](exec/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-exec-coverage.json) |
<!-- module-table:end -->

</details>
