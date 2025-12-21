# gglib-hf

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-complexity.json)

`HuggingFace` Hub client implementation for gglib.

## Architecture

This crate is in the **Infrastructure Layer** — it implements the `HfClientPort` trait from `gglib-core`.

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Overview

This crate provides a complete `HuggingFace` Hub API client for searching, browsing, and downloading GGUF models. It implements the `HfClientPort` trait from `gglib-core`, following the Hexagonal Architecture pattern.

## Architecture

```text
gglib-core (port definition)        gglib-hf (adapter implementation)
┌─────────────────────────────┐     ┌────────────────────────────────┐
│  HfClientPort trait         │◄────│  impl HfClientPort for HfClient│
│  HfSearchOptions            │     │  DefaultHfClient type alias    │
│  HfSearchResponse           │     │  HfClientConfig                │
│  HfModelInfo                │     │  Internal: HttpBackend,        │
│  HfQuantizationInfo         │     │            parsing, url, models│
│  HfClientError              │     └────────────────────────────────┘
└─────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](src/config) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-config-coverage.json) |
| [`error.rs`](src/error) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-error-coverage.json) |
| [`http.rs`](src/http) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-http-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-http-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-http-coverage.json) |
| [`models.rs`](src/models) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-models-coverage.json) |
| [`parsing.rs`](src/parsing) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-parsing-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-parsing-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-parsing-coverage.json) |
| [`port.rs`](src/port) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-port-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-port-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-port-coverage.json) |
| [`url.rs`](src/url) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-url-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-url-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-url-coverage.json) |
| [`client/`](src/client/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-client-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`config.rs`** — Client configuration (tokens, base URLs)
- **`error.rs`** — Error types for API failures
- **`http.rs`** — HTTP backend abstraction for testability
- **`models.rs`** — Response models and deserialization
- **`parsing.rs`** — HTML/JSON parsing for model pages
- **`port.rs`** — `HfClientPort` trait implementation
- **`url.rs`** — URL construction helpers
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

- **Model Search**: Search `HuggingFace` Hub for GGUF models with pagination and sorting
- **Quantization Listing**: List available quantization variants (`Q4_K_M`, `Q5_K_S`, etc.)
- **File Resolution**: Find specific GGUF files for download, including sharded models
- **Commit SHA Lookup**: Get latest commit SHA for version tracking
- **Authenticated Access**: Optional `HuggingFace` token for gated models

## Design Decisions

1. **Port Pattern**: Core owns the trait and DTOs, this crate implements the adapter
2. **Generic HTTP Backend**: `HfClient<B: HttpBackend>` allows testing with mock backends
3. **No Re-exports**: Types flow through `gglib-core` ports, not transitional aliases
4. **Split Modules**: Each module under 200 lines for maintainability
