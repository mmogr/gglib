# detect

<!-- module-docs:start -->

Hardware acceleration detection for llama.cpp builds.

This module detects which GPU acceleration backend (Metal, CUDA, or
Vulkan) is available on the current system and selects the optimal
one for building llama.cpp.

# Submodules

| Module | Responsibility |
|--------|---------------|
| [`tools`] | Shared command-execution and version-parsing utilities |
| [`metal`] | Apple Metal detection (macOS only) |
| [`cuda`] | NVIDIA CUDA toolkit detection and GCC compatibility |
| [`vulkan`] | Vulkan loader, header, and `glslc` detection |

# Priority order

[`detect_optimal_acceleration`] selects backends in this priority:

1. **Metal** — macOS with Apple Silicon or Intel Mac ≥10.13
2. **CUDA** — NVIDIA GPU with `nvcc` in `PATH`
3. **Vulkan** — AMD/Intel/NVIDIA via portable GPU API (runtime only)

CPU-only inference is not supported.

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
