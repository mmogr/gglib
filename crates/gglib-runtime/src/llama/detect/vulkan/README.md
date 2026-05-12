# vulkan

<!-- module-docs:start -->

Vulkan acceleration detection with build-readiness validation.

Vulkan is a portable GPU API supported on AMD, Intel, and NVIDIA hardware
across Linux and Windows. This module determines whether all components
needed for a `-DGGML_VULKAN=ON` llama.cpp build are present on the current
system.

## Platform scope

Vulkan probing is **only compiled on Linux and Windows**. On macOS, the
`vulkan_status()` function returns `VulkanStatus::absent()` directly —
this is not an error. macOS uses Metal as its native GPU API, and Metal
detection lives in the sibling `metal` module.

The platform isolation is enforced at the module declaration level in
`mod.rs`, not with scattered `#[allow(dead_code)]` attributes:

```rust
// In mod.rs:
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod probe;          // ← never compiled on macOS

pub fn vulkan_status() -> VulkanStatus {
    // macOS: single-line stub, no dead helpers compiled
    VulkanStatus::absent()
}
```

## Cross-platform requirements

| Platform | Loader | Dev Headers | glslc | SPIR-V Headers | Result |
|----------|--------|-------------|-------|----------------|--------|
| **Linux** | `libvulkan.so.1` | `vulkan/vulkan.h` | `glslc` | `spirv/unified1/spirv.hpp` | Probed at runtime |
| **Windows** | `vulkan-1.dll` | LunarG Vulkan SDK | LunarG SDK | LunarG SDK | Probed at runtime |
| **macOS** | — | — | — | — | `absent()` stub, Metal used instead |

## File layout

| File | Compiled on | Responsibility |
|------|-------------|---------------|
| `types.rs` | All platforms | `MissingPackage`, `VulkanStatus`, `VulkanStatus::absent()` |
| `probe.rs` | Linux + Windows | Component probes, `pkg-config` queries, `header_exists_in` helper |
| `mod.rs` | All platforms | Public facade — dispatches to `probe` or returns `absent()` |

## Components checked (Linux/Windows)

`VulkanStatus` captures each component independently for targeted
remediation advice:

1. **Vulkan loader** — `vulkaninfo --summary`, fallback disk paths
2. **Vulkan headers** — `pkg-config`, fallback `/usr/include`
3. **glslc** — `glslc --version` (SPIR-V shader compiler, separate from headers)
4. **SPIR-V headers** — `pkg-config SPIRV-Headers`, fallback disk paths

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`probe.rs`](probe.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-probe-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-probe-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-probe-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-vulkan-types-coverage.json) |
<!-- module-table:end -->

</details>
