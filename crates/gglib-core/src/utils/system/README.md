# system

<!-- module-docs:start -->

System utility types for dependency and environment detection.

This module provides pure domain types for system dependencies,
GPU information, and memory details. Active system probing is
implemented by `DefaultSystemProbe` in `gglib-runtime`.

# Architecture Note

Core defines types + the `SystemProbePort` trait (in `ports::system_probe`).
Runtime implements `DefaultSystemProbe` which performs actual system queries.

`packages` follows the same split, which is what keeps it here rather than in
the runtime: `parse_os_release` takes the *contents* of `/etc/os-release` and
returns a `LinuxDistro`, so the decision is pure and testable against files
from distributions no CI machine runs, while the `std::fs` call that produces
those contents lives in `gglib_runtime::system::detect_linux_distro`.

It is the single source of truth for what each dependency is called on each
distribution — used by the dependency probe's install hints, by
`gglib config check-deps`, and by the llama.cpp build's own prerequisite
message, which previously each carried their own copy and had drifted apart.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`packages.rs`](packages.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-packages-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-packages-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-packages-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-coverage.json) |
<!-- module-table:end -->

</details>
