# gglib-gguf

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-complexity.json)
<!-- crate-header:end -->

GGUF file format parser for gglib — extracts metadata from GGUF model files.

What this crate reads out of a file is what lets the runtime configure it
without asking: architecture, chat template, and layer counts become capability
tags, and those tags become llama-server flags. No per-model setup.

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

**Module Descriptions:**
- **`error.rs`** — Parser error types
- **`fixture.rs`** — `write_string_gguf`, `write_tensor_gguf` and `write_safetensors`, weights files written without a model for a test to parse; behind the `test-utils` feature
- **`format.rs`** — GGUF format types, tensor types, and quantization enums
- **`head.rs`** — A GGUF file's metadata from its first bytes, as far as they go (`GgufParserPort::parse_head`), so a download can be judged before it starts: the pairs the head holds whole are kept, and the head ending, or a size the file declares running past it, is no error. Only a head without the GGUF magic, or with a malformed value, is refused
- **`parser.rs`** — High-level metadata extraction and port implementation
- **`reader.rs`** — Low-level I/O over a file or a head of one, and GGUF header parsing; a string length, array count or metadata count the file declares is held to the bytes it has left before anything is reserved or looped over
- **`role.rs`** — Whether the header says the file is a model's weights or a projector
- **`tensor_table.rs`** — A weights file's tensor table (`GgufParserPort::tensor_table` and `tensor_table_of_head`): every tensor's name and shape, outermost first, from a GGUF header or, when the first four bytes are not the GGUF magic, a safetensors header. The tensor count is held to the bytes left and a tensor of more than four dimensions refused before anything is reserved
- **`safetensors.rs`** — A safetensors header as a tensor table; a header length over 100,000,000 bytes or over what the file holds is refused before anything is reserved
- **`capabilities/`** — Model capability detection (context size, chat templates)
- **`render/`** — `render_chat`: a chat rendered through its template the way the pinned llama-server renders it, held byte for byte to that server's recorded renders
- **`template_env.rs`** — The minijinja environment `capabilities/` and `render/` both start from: Python's string methods and `raise_exception`

## Features

- **GGUF v3 Support** — Parses the latest GGUF file format
- **Metadata Extraction** — Extracts model name, architecture, context size, quantization
- **Head Parsing** — Reads the metadata a file's first bytes hold, such as a 64 KiB range of a file on the Hub
- **Capability Detection** — Identifies chat template, vocabulary size, embedding dimensions
- **Efficient Parsing** — Streams metadata without loading full tensor data

## Capability Tags

Model capabilities are detected from GGUF metadata and stored as string tags.
These tags drive automatic llama-server flag selection at serve time.

| Tag | Detection trigger | Effect at runtime |
|-----|-------------------|-------------------|
| `"agent"` | Chat template contains tool-calling syntax | `--jinja` auto-enabled |
| `"reasoning"` | Chat template contains `<think>` / `DeepSeek` reasoning tokens | `--reasoning-format deepseek` auto-enabled |
| `"mtp"` | `{arch}.nextn_predict_layers > 0` in GGUF metadata | `--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75` auto-enabled |
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

    println!("Architecture: {:?}", metadata.architecture);
    println!("Context size: {}", metadata.context_length.unwrap_or(0));
    println!("Quantization: {:?}", metadata.quantization);
}
```

A chat rendered as the pinned llama-server renders it, with the special
tokens the loaded model's vocabulary gives:

```rust
use gglib_gguf::{ChatKwargs, ChatMessage, ChatTemplate, VocabTokens, render_chat};
use serde_json::json;

let vocab = VocabTokens { bos: "<s>".into(), eos: "</s>".into(), add_bos: true, add_eos: false };
let template = ChatTemplate::new("", vocab).unwrap(); // no template: llama-server's ChatML
let kwargs = ChatKwargs::from_request(&json!({"reasoning_budget_tokens": 0})).unwrap();
let prompt = render_chat(&template, &[ChatMessage::new("user", "hi")], true, &kwargs).unwrap();
assert_eq!(prompt, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n");
```

## Design Decisions

1. **Port Pattern** — Implements `GgufParser` trait from `gglib-core`
2. **No Full Load** — Only parses header and metadata, not tensor weights
3. **Quantization Detection** — Infers quantization from filename patterns and metadata
