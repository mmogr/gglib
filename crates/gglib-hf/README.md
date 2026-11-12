# gglib-hf

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-complexity.json)

Layer: **infrastructure** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

`HuggingFace` Hub client for gglib — searching, browsing, and resolving GGUF models on the Hub. Implements `HfClientPort` from `gglib-core`.

Resolving a repository to the right quantization is the step that turns "I want
this model" into a concrete file to fetch.

## Architecture

This crate is in the **Infrastructure Layer** — it implements the `HfClientPort` trait from `gglib-core`.

## Internal Structure

```text
gglib-core (port definition)        gglib-hf (adapter implementation)
┌─────────────────────────────┐     ┌────────────────────────────────┐
│  HfClientPort trait         │◄────│  impl HfClientPort for HfClient│
│  HfSearchOptions            │     │  DefaultHfClient type alias    │
│  HfSortField                │     │  HfClientConfig                │
│  HfSearchResult             │     │  Internal: HttpBackend,        │
│  HfRepoInfo                 │     │            parsing, url, models│
│  HfQuantInfo, HfFileInfo    │     └────────────────────────────────┘
│  HfPortError                │
└─────────────────────────────┘
```

**Module Descriptions:**
- **`config.rs`** — Client configuration (tokens, base URLs)
- **`error.rs`** — Error types for API failures
- **`file_roles.rs`** — Finding a repository's projectors by name
- **`http.rs`** — HTTP backend abstraction for testability: JSON reads, a ranged read of a file's head, and a posted form
- **`kind.rs`** — What each `HfModelKind` of search is: the tags it asks the Hub for, and which hits it keeps
- **`models.rs`** — What the crate holds of a repository: its reference and its file entries
- **`parsing.rs`** — JSON parsing: a model's summary (`repo_info_from_json`, the one reader of a search hit and of a model info), a file tree, and the grouping of files into labelled variants
- **`port.rs`** — `HfClientPort` trait implementation
- **`url.rs`** — URL construction helpers and the Hub's own name for each `HfSortField`
- **`client/`** — HTTP client implementation and `HuggingFace` API integration

## Usage

```rust,no_run
use gglib_core::ports::huggingface::{HfClientPort, HfSearchOptions};
use gglib_hf::{DefaultHfClient, HfClientConfig};

async fn example() {
    // Create a client with optional authentication
    let config = HfClientConfig::new().with_token("hf_...");
    let client = DefaultHfClient::new(&config);

    // Search for models
    let options = HfSearchOptions {
        query: Some("llama".to_string()),
        limit: 10,
        ..Default::default()
    };
    let response = client.search(&options).await.unwrap();

    // List available quantizations
    let quantizations = client.list_quantizations("TheBloke/Llama-2-7B-GGUF").await.unwrap();
}
```

## Features

- **Model Search**: Search `HuggingFace` Hub for GGUF models with pagination and sorting, of
  models that chat (`filter=gguf`, keeping the hits whose pipeline is a chat one or none) or
  text-to-image ones (`filter=gguf&filter=text-to-image`)
- **Variant Listing**: A repository's variants: its GGUF files grouped into shard sets,
  each labelled as its uploader named it (`UD-Q4_K_XL`, `Q3_K_P`, `FastMTP-32K`), a quant
  spelt in capitals (`Q6_k` is `Q6_K`), with no quant read from a name, so no two distinct
  files are merged as shards
- **Projector Listing**: A repository's projectors (`mmproj` files) are listed apart, with
  their OIDs. A projector is a variant of its own, never a shard of another
- **File Resolution**: Find specific GGUF files for download, including sharded models
- **One File by Path**: A file's first bytes, read with a `Range` from the address a download
  fetches it from, and its size and LFS OID, looked up at `paths-info`
- **Commit SHA Lookup**: Get latest commit SHA for version tracking
- **Authenticated Access**: Optional `HuggingFace` token for gated models

## Design Decisions

1. **Port Pattern**: Core owns the trait and DTOs, this crate implements the adapter
2. **Generic HTTP Backend**: `HfClient<B: HttpBackend>` allows testing with mock backends
3. **No Re-exports**: Types flow through `gglib-core` ports, not transitional aliases
4. **Split Modules**: One module per concern, as listed above
