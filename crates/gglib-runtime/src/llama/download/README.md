# download

<!-- module-docs:start -->

Pre-built llama.cpp binary download support.

This module handles downloading pre-built llama.cpp binaries from GitHub releases
for users running pre-built gglib binaries (not building from source).

Platform support:
- macOS ARM64: Metal-enabled pre-built binaries
- macOS x64: Metal-enabled pre-built binaries
- Windows x64: CUDA or Vulkan pre-built binaries (selected at runtime via GPU detection)
- Linux x64: CPU pre-built binaries (CUDA requires building from source)

`download_prebuilt_binaries` emits [`LlamaProgressEvent`](super::install_events::LlamaProgressEvent)
on a `tokio::sync::mpsc::Sender` and is consumed by:

| Consumer | Output                                                 |
|----------|--------------------------------------------------------|
| CLI      | `indicatif` progress bar                               |
| Axum     | SSE stream at `POST /api/config/system/install-llama`  |
| Tauri    | `llama-install-progress` event to the WebView          |

It is **not** responsible for rendering: no `println!`, no progress bar, no
knowledge of a terminal, an HTTP response or a WebView. Rate and ETA are
measured here so that no surface has to derive them.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
