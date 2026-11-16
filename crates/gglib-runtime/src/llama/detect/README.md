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
3. **Vulkan** — AMD/Intel/NVIDIA via portable GPU API, every build package present
4. **CPU** — no GPU at all: the build runs, slowly, and says so

A Vulkan GPU whose build packages are missing is refused, with the
packages to install, rather than built for the CPU. Whether a Vulkan GPU
is there is decided once, in `system::vulkan_devices`, for this choice and
the downloads alike: a GPU the Vulkan driver lists, or one the operating
system shows with a Vulkan driver, not only the Vulkan loader, which many
systems install with no GPU or driver for it to drive.

<!-- module-docs:end -->
