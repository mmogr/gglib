# contracts

<!-- module-docs:start -->

Transport contract constants shared across adapters.

This module contains string constants for API routes and command names used by both Axum (HTTP) and Tauri (IPC) adapters. By keeping these as pure string constants with no framework-specific types, we avoid dependency creep and ensure consistency.

## Design

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                           contracts/                                                │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌──────────────────────────────────────────────────────────────────────────────┐   │
│  │  http/                                                                       │   │
│  │  HTTP route constants: "/api/models", "/api/chat/completions", etc.         │   │
│  └──────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Rules

- **String-only** — No framework types (no `axum::Router`, no `tauri::Command`)
- **No logic** — Pure constants, no validation or parsing
- **Single source of truth** — Both adapters import from here

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`http/`](http/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-coverage.json) |
<!-- module-table:end -->

</details>
