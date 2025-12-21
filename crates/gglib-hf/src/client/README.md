# client

<!-- module-docs:start -->

HuggingFace client for searching models and fetching metadata.

Provides the main client interface for interacting with the HuggingFace Hub API.

## Key Types

| Type | Description |
|------|-------------|
| `HfClient<B>` | Generic client over HTTP backend |
| `DefaultHfClient` | Production client using reqwest |

## Submodules

| Module | Description |
|--------|-------------|
| `repo_files` | Repository file listing |
| `search` | Model search queries |

## Design

The client is generic over an HTTP backend (`B: HttpBackend`), allowing easy testing with mock backends. Use `DefaultHfClient` for production.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`repo_files.rs`](repo_files.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-repo_files-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-repo_files-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-repo_files-coverage.json) |
| [`search.rs`](search.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-search-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-search-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-search-coverage.json) |
<!-- module-table:end -->

</details>
