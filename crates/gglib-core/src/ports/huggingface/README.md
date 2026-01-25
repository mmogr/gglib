# huggingface

<!-- module-docs:start -->

HuggingFace Hub client port definition.

Defines the `HfClientPort` trait for interacting with the HuggingFace Hub API. The actual implementation lives in `gglib-hf`.

## Trait: `HfClientPort`

| Method | Description |
|--------|-------------|
| `search_models()` | Search for GGUF models on HuggingFace |
| `get_repo_info()` | Get repository metadata |
| `list_files()` | List files in a repository |
| `get_download_url()` | Generate authenticated download URL |

## Types

| Type | Description |
|------|-------------|
| `HfRepoInfo` | Repository metadata (name, downloads, likes) |
| `HfFileInfo` | File metadata (name, size, SHA) |
| `HfQuantInfo` | Quantization info parsed from filename |
| `HfSearchResult` | Paginated search results |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`client.rs`](client.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-client-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-client-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-client-coverage.json) |
| [`error.rs`](error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-error-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-types-coverage.json) |
<!-- module-table:end -->

</details>
