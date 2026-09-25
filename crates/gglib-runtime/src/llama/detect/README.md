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
