# install

<!-- module-docs:start -->

Installation command for llama.cpp.

Orchestrates the full installation flow, choosing between pre-built download or source build based on context.

## Installation Methods

| Context | Method |
|---------|--------|
| `--build` flag | Always build from source |
| Running from source repo | Build from source |
| Pre-built binary + macOS/Windows | Download pre-built |
| Linux | Build from source (CUDA support) |

## Flow

```text
install command
      │
      ├─── Pre-built available? ──▶ download/
      │
      └─── Build from source ───▶ deps check ─▶ build/
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
