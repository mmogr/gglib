# args

<!-- module-docs:start -->

Shared helpers for building llama.cpp invocations.

Reusable utilities for resolving CLI flags and configuration options so that multiple commands can stay DRY.

## Submodules

| Module | Description |
|--------|-------------|
| `context` | Context size resolution (auto-detect from model or user override) |
| `jinja` | Jinja template flag resolution |
| `reasoning` | Reasoning format detection and flag resolution |

## Key Functions

- `resolve_context_size()` — Determine optimal context size
- `resolve_jinja_flag()` — Whether to enable Jinja templates
- `resolve_reasoning_format()` — Detect and configure reasoning models

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`context.rs`](context) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-coverage.json) |
| [`jinja.rs`](jinja) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-coverage.json) |
| [`reasoning.rs`](reasoning) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-coverage.json) |
<!-- module-table:end -->

</details>
