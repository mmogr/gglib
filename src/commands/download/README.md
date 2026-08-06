<!-- module-docs:start -->

# Download Module

The download module handles interactions with the HuggingFace Hub, including searching, browsing, and downloading GGUF models.

## Architecture

```text
┌─────────────┐      ┌────────────────┐      ┌──────────────────┐
│ User Request│ ───► │   HuggingFace  │ ───► │   Quantization   │
│ (CLI/GUI)   │      │      API       │      │      Filter      │
└─────────────┘      └───────┬────────┘      └────────┬─────────┘
                             │                        │
                             ▼                        ▼
                     ┌────────────────┐      ┌──────────────────┐
                     │    File Ops    │ ◄─── │    Model Ops     │
                     │ (Write to Disk)│      │ (Verify/Process) │
                     └────────────────┘      └──────────────────┘
```

## Components

- **api.rs**: Handles HTTP requests to the HuggingFace Hub API.
- **file_ops.rs**: Manages file system operations, including downloading and verifying files.
- **model_ops.rs**: Processes model metadata, handles database insertion, and auto-detects reasoning models.
- **progress.rs**: Provides progress bars and status updates during downloads.
- **python_bridge.rs**: Spins up the managed Python helper (hf_xet) for accelerated transfers and streams progress back as JSON events.
- **utils.rs**: Utility functions for the download module.

### Reasoning Model Detection

When a GGUF file is downloaded and added to the database, `model_ops.rs` automatically analyzes the model's metadata to detect reasoning/thinking capabilities. Models with chat templates containing `<think>`, `<reasoning>`, or similar tags (e.g., DeepSeek R1, Qwen3, QwQ) receive a `reasoning` tag automatically. This enables optimal configuration when serving via `llama-server --reasoning-format`.

### Download backends

Downloads run natively over HTTP by default: a `reqwest`-based streaming downloader with resumable `.part` transfers, SHA-256 verification against `X-Linked-Etag`, and atomic rename on completion. No Python is required for a download to succeed.

The `hf_xet` accelerator is opt-in. When its environment has already been provisioned (`gglib config fast-downloads enable`, or by accepting the offer that `make setup` and `gglib up` make), the flow invokes `hf_xet_downloader.py` inside the managed environment (`<data_root>/.python/gglib-hf-xet`, with `huggingface_hub>=1.1.5` and `hf_xet>=0.6`), which pulls GGUF blobs via Xet storage and emits newline-delimited JSON progress that ties back into the existing `ProgressCallback` plumbing. The environment is never provisioned implicitly, and if the accelerator is present but fails, the download falls back to the native path rather than erroring.

## Deep Dive: Quantization Filter

When a user requests a model (e.g., "TheBloke/Llama-2-7B-Chat-GGUF"), the repository may contain dozens of files. The download module applies a heuristic to select the best default:

1.  **User Preference**: If the user specifies `--quantization Q4_K_M`, we look for that exact string.
2.  **Recommended Defaults**: If no preference is given, we prioritize balanced quantizations in this order: `Q5_K_M`, `Q4_K_M`, `Q5_K_S`, `Q4_K_S`.
3.  **Fallback**: If none of the preferred types are found, we fall back to the smallest available file to save bandwidth, or prompt the user (interactive mode).

<!-- module-docs:end -->
