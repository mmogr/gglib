# dto

<!-- module-docs:start -->

Data Transfer Objects (DTOs) for HTTP API contract.

These types define the stable HTTP API contract with explicit serialization control, decoupling internal domain types from external API representation.

## Purpose

- **API stability** — Internal domain changes don't break clients
- **Explicit serialization** — Full control over JSON field names
- **Validation** — Input validation at API boundary

## Submodules

| Module | Description |
|--------|-------------|
| `system` | System info DTOs (`SystemMemoryInfoDto`) |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`system.rs`](system) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-system-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-system-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-dto-system-coverage.json) |
<!-- module-table:end -->

</details>
