# args

<!-- module-docs:start -->

Shared helpers for building llama.cpp invocations.

Reusable utilities for resolving CLI flags and configuration options so that multiple commands can stay DRY.

## Submodules

| Module | Description |
|--------|-------------|
| `context` | Context size resolution (auto-detect from model or user override) |
| `jinja` | Jinja template flag resolution |
| `mtp` | MTP (Multi-Token Prediction) speculative decoding arg resolution |
| `reasoning` | Reasoning format detection and flag resolution |

## Key Functions

- `resolve_context_size(ContextInput)` — Determine context size via the 3-level
  fallback: explicit flag → global settings default → llama-server default
- `resolve_jinja_flag()` — Whether to enable Jinja templates
- `resolve_mtp_args(explicit_n, explicit_p, tags)` — Resolve `--spec-type draft-mtp`
  arguments: auto-enables with n=2, p=0.75 when the `"mtp"` tag is present;
  explicit n=0 disables even on tagged models
- `resolve_reasoning_format()` — Detect and configure reasoning models

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`context.rs`](context.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-context-coverage.json) |
| [`jinja.rs`](jinja.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-jinja-coverage.json) |
| [`mtp.rs`](mtp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-mtp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-mtp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-mtp-coverage.json) |
| [`reasoning.rs`](reasoning.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-args-reasoning-coverage.json) |
<!-- module-table:end -->

</details>
