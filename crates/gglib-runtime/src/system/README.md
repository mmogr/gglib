# system

<!-- module-docs:start -->

System probe implementation.

Provides `DefaultSystemProbe` which implements `SystemProbePort` from gglib-core. Performs active system probing via command execution and hardware detection.

## Capabilities

| Capability | Description |
|------------|-------------|
| GPU detection | Metal (macOS), CUDA (Linux/Windows) |
| Memory info | Total and available system memory |
| Dependencies | Check for cmake, git, gcc, rustc, etc. |

## Submodules

- `commands` — Command execution for version detection
- `deps` — Library dependency checks (libssl, webkit2gtk, etc.)
- `gpu` — GPU and memory detection

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`commands.rs`](commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-commands-coverage.json) |
| [`deps.rs`](deps.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-deps-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-deps-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-deps-coverage.json) |
| [`gpu.rs`](gpu.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-gpu-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-gpu-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-gpu-coverage.json) |
<!-- module-table:end -->

</details>
