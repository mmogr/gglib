# system

<!-- module-docs:start -->

System utility types for dependency and environment detection.

This module provides pure domain types for system dependencies,
GPU information, and memory details. Active system probing is
implemented by `DefaultSystemProbe` in `gglib-runtime`.

# Architecture Note

Core defines types + the `SystemProbePort` trait (in `ports::system_probe`).
Runtime implements `DefaultSystemProbe` which performs actual system queries.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-system-types-coverage.json) |
<!-- module-table:end -->

</details>
