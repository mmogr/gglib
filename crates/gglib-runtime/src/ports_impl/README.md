# ports_impl

<!-- module-docs:start -->

Port implementations for gglib-runtime.

These implementations provide concrete adapters for the abstract ports defined in gglib-core, connecting port interfaces to actual runtime infrastructure.

## Implementations

| Implementation | Port | Description |
|----------------|------|-------------|
| `CatalogPortImpl` | `ModelCatalogPort` | Model lookup from database |
| `RuntimePortImpl` | `ModelRuntimePort` | Process lifecycle management |

## Design

These adapters bridge the gap between abstract ports (defined in core) and concrete infrastructure (ProcessManager, database connections).

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`model_catalog.rs`](model_catalog.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_catalog-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_catalog-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_catalog-coverage.json) |
| [`model_runtime.rs`](model_runtime.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_runtime-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_runtime-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-model_runtime-coverage.json) |
<!-- module-table:end -->

</details>
