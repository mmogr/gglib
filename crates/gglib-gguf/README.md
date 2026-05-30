# gglib-gguf

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-complexity.json)

GGUF file format parser for gglib — extracts metadata from GGUF model files.

## Architecture

This crate is in the **Infrastructure Layer** — it implements the `GgufParser` port from `gglib-core`.

```text
gglib-core (port)           gglib-gguf (adapter)          Adapters
┌──────────────────┐        ┌──────────────────┐        ┌──────────────────┐
│   GgufParser     │◄───────│  impl GgufParser │◄───────│    gglib-cli     │
│   trait          │        │  for GgufReader  │        │   gglib-axum     │
└──────────────────┘        └──────────────────┘        └──────────────────┘
                                     │
                                     ▼
                            ┌──────────────────┐
                            │   .gguf files    │
                            │ (GGUF v3 format) │
                            └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-gguf                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │  reader.rs  │ ──► │  parser.rs  │ ──► │  format.rs  │     │capabilities/│        │
│  │ File I/O &  │     │  Metadata   │     │ GGUF types  │     │  Model caps │        │
│  │   header    │     │  extraction │     │  & enums    │     │  detection  │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌─────────────┐                                                                    │
│  │  error.rs   │                                                                    │
│  │ Parse errors│                                                                    │
│  └─────────────┘                                                                    │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                depends on
                                          ▼
                              ┌───────────────────┐
                              │    gglib-core     │
                              │  (GgufMetadata)   │
                              └───────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-error-coverage.json) |
| [`format.rs`](src/format.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-format-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-format-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-format-coverage.json) |
| [`parser.rs`](src/parser.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-parser-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-parser-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-parser-coverage.json) |
| [`reader.rs`](src/reader.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-reader-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-reader-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-reader-coverage.json) |
| [`validation.rs`](src/validation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-validation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-validation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-validation-coverage.json) |
| [`capabilities/`](src/capabilities/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`error.rs`** — Parser error types
- **`format.rs`** — GGUF format types, tensor types, and quantization enums
- **`parser.rs`** — High-level metadata extraction and port implementation
- **`reader.rs`** — Low-level file I/O and GGUF header parsing
- **`capabilities/`** — Model capability detection (context size, chat templates)

## Features

- **GGUF v3 Support** — Parses the latest GGUF file format
- **Metadata Extraction** — Extracts model name, architecture, context size, quantization
- **Capability Detection** — Identifies chat template, vocabulary size, embedding dimensions
- **Efficient Parsing** — Streams metadata without loading full tensor data

## Capability Tags

Model capabilities are detected from GGUF metadata and stored as string tags.
These tags drive automatic llama-server flag selection at serve time.

| Tag | Detection trigger | Effect at runtime |
|-----|-------------------|-------------------|
| `"agent"` | Chat template contains tool-calling syntax | `--jinja` auto-enabled |
| `"reasoning"` | Chat template contains `<think>` / DeepSeek reasoning tokens | `--reasoning-format deepseek` auto-enabled |
| `"mtp"` | `{arch}.nextn_predict_layers > 0` in GGUF metadata | `--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75` auto-enabled |
| `"vision"` | Multi-modal clip projection keys present | Informational only (future) |
| `"moe"` | `{arch}.expert_count > 0` | Informational only |

### MTP tag details

The `"mtp"` tag is set when the GGUF file contains the key
`{arch}.nextn_predict_layers` (e.g. `qwen3_5_mtp.nextn_predict_layers`) with a
value strictly greater than zero.  This key is written by llama.cpp when MTP
draft head tensors are bundled into the same file.

**Detection is intentionally strict**: model names or filenames containing
`"MTP"` are ignored.  A model whose MTP heads have been stripped during
quantisation will NOT receive the tag, preventing llama-server from being
launched with `--spec-type draft-mtp` against a file that cannot support it.

CLI escape hatch: `gglib serve <id> --mtp-draft-n-max 0` explicitly disables
MTP even on a tagged model.

## Usage

```rust,no_run
use gglib_gguf::GgufParser;
use gglib_core::ports::GgufParserPort;
use std::path::Path;

fn example() {
    let parser = GgufParser;
    let metadata = parser.parse(Path::new("/path/to/model.gguf")).unwrap();

    println!("Model: {}", metadata.name.as_deref().unwrap_or("Unknown"));
    println!("Architecture: {:?}", metadata.architecture);
    println!("Context size: {}", metadata.context_length.unwrap_or(0));
    println!("Quantization: {:?}", metadata.quantization);
}
```

## Design Decisions

1. **Port Pattern** — Implements `GgufParser` trait from `gglib-core`
2. **No Full Load** — Only parses header and metadata, not tensor weights
3. **Quantization Detection** — Infers quantization from filename patterns and metadata
