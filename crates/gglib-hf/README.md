# gglib-hf

HuggingFace Hub client implementation for gglib.

## Overview

This crate provides a complete HuggingFace Hub API client for searching, browsing, and downloading GGUF models. It implements the `HfClientPort` trait from `gglib-core`, following the Hexagonal Architecture pattern.

## Architecture

```
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

## Usage

```rust
use gglib_core::ports::huggingface::{HfClientPort, HfSearchOptions};
use gglib_hf::{DefaultHfClient, HfClientConfig};

// Create a client with optional authentication
let config = HfClientConfig::new().with_token("hf_...");
let client = DefaultHfClient::with_config(config);

// Search for models
let options = HfSearchOptions {
    query: Some("llama".to_string()),
    limit: Some(10),
    ..Default::default()
};
let response = client.search(&options).await?;

// List available quantizations
let quantizations = client.list_quantizations("TheBloke/Llama-2-7B-GGUF").await?;
```

## Features

- **Model Search**: Search HuggingFace Hub for GGUF models with pagination and sorting
- **Quantization Listing**: List available quantization variants (Q4_K_M, Q5_K_S, etc.)
- **File Resolution**: Find specific GGUF files for download, including sharded models
- **Commit SHA Lookup**: Get latest commit SHA for version tracking
- **Authenticated Access**: Optional HuggingFace token for gated models

## Design Decisions

1. **Port Pattern**: Core owns the trait and DTOs, this crate implements the adapter
2. **Generic HTTP Backend**: `HfClient<B: HttpBackend>` allows testing with mock backends
3. **No Re-exports**: Types flow through `gglib-core` ports, not transitional aliases
4. **Split Modules**: Each module under 200 lines for maintainability
