# capabilities

<!-- module-docs:start -->

Model capability detection.

This module analyzes GGUF metadata to detect model capabilities
such as reasoning/thinking support and tool/function calling.

# Structure

- `reasoning` - Reasoning/thinking model detection
- `tool_calling` - Tool/function calling detection
- `patterns` - Pattern constants shared across detection modules

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`mtp.rs`](mtp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-mtp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-mtp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-mtp-coverage.json) |
| [`patterns.rs`](patterns.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-patterns-coverage.json) |
| [`reasoning.rs`](reasoning.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-reasoning-coverage.json) |
| [`tool_calling.rs`](tool_calling.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-tool_calling-coverage.json) |
<!-- module-table:end -->

</details>
