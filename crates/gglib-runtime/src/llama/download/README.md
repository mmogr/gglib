# download

<!-- module-docs:start -->

Pre-built llama.cpp binary download support.

This module downloads the pinned llama.cpp release's pre-built binaries from
GitHub, which is how every install goes, from a checkout too, unless a source
build is asked for or no download fits (`gglib_runtime::engine`).

Platform support (`llama_platform_asset`):
- macOS ARM64 and x64: Metal
- Linux x64 and ARM64: Vulkan when a Vulkan GPU is there, else the CPU build,
  with a warning (CUDA is a source build, with `--build`)
- Windows x64: CUDA 12.4 for an NVIDIA GPU with CUDA, else Vulkan, else the
  CPU build, with a warning; Windows ARM64: Vulkan, else the CPU build

This module holds what is llama.cpp's own: the pin and its override
(`GGLIB_LLAMA_RELEASE`), the platform table, the CUDA runtime package, and
the `ReleaseSpec` the shared installer in `binary_install` runs.

`download_prebuilt_binaries` emits [`LlamaProgressEvent`](super::install_events::LlamaProgressEvent)
on a `tokio::sync::mpsc::Sender` and is consumed by:

| Consumer | Output                                                        |
|----------|---------------------------------------------------------------|
| CLI      | `indicatif` progress bar                                      |
| Axum     | SSE stream at `POST /api/config/system/engines/llama/install` |

The desktop app reads the Axum stream; it has no consumer of its own.

It is **not** responsible for rendering: no `println!`, no progress bar, no
knowledge of a terminal, an HTTP response or a `WebView`. Rate and ETA are
measured here so that no surface has to derive them.

<!-- module-docs:end -->
