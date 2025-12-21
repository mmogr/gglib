# capabilities

<!-- module-docs:start -->

Model capability detection from GGUF metadata.

Analyzes GGUF metadata to detect model capabilities such as reasoning/thinking support and tool/function calling.

## Detected Capabilities

| Capability | Detection Method |
|------------|------------------|
| Reasoning | Template patterns, model name heuristics |
| Tool Calling | Template `tools` blocks, Hermes/Functionary patterns |
| Vision | (planned) multimodal architecture detection |

## Submodules

| Module | Description |
|--------|-------------|
| `reasoning` | Reasoning/thinking model detection |
| `tool_calling` | Tool/function calling detection |
| `patterns` | Shared pattern constants |

## Entry Point

`detect_all(metadata)` → `GgufCapabilities` with flags and extensions.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`patterns.rs`](patterns) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-coverage.json) |
| [`reasoning.rs`](reasoning) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-coverage.json) |
| [`tool_calling.rs`](tool_calling) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-coverage.json) |
<!-- module-table:end -->

</details>
