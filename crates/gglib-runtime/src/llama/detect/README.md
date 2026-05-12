# detect

<!-- module-docs:start -->

Hardware acceleration detection for llama.cpp builds.

This module probes the current system to determine which GPU backend
(Metal, CUDA, or Vulkan) is available and fully build-ready. It is the
authoritative source for acceleration decisions used by the CLI,
REST API, and Tauri GUI — all three surfaces call
`detect_optimal_acceleration()` and `vulkan_status()` from here.

## Priority order

`detect_optimal_acceleration()` returns the first fully-buildable backend:

| Priority | Backend | Platform |
|----------|---------|----------|
| 1 | Metal | macOS (Apple Silicon or Intel Mac ≥ 10.13) |
| 2 | CUDA | Any platform with `nvcc` in `PATH` |
| 3 | Vulkan | Linux or Windows with loader + headers + `glslc` + SPIR-V headers |

CPU-only inference is not supported; the function returns `Err` if no
GPU backend is fully buildable so callers can surface install hints.

## Submodules

| Submodule | Responsibility |
|-----------|---------------|
| [`tools`](tools) | Shared command-execution and version-parsing helpers |
| [`metal`](metal) | Apple Metal detection (macOS only) |
| [`cuda`](cuda) | NVIDIA CUDA toolkit detection and GCC compatibility |
| [`vulkan/`](vulkan/) | Vulkan loader, header, `glslc`, and SPIR-V detection (Linux/Windows) |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`cuda.rs`](cuda.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-cuda-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-cuda-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-cuda-coverage.json) |
| [`metal.rs`](metal.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-metal-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-metal-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-metal-coverage.json) |
| [`tools.rs`](tools.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-tools-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-tools-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-detect-tools-coverage.json) |
| [`vulkan/`](vulkan/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-coverage.json) |
<!-- module-table:end -->

</details>
