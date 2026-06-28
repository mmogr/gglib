# proxy

<!-- module-docs:start -->

OpenAI-compatible proxy module.

This module provides the proxy supervisor for managing the OpenAI-compatible
proxy server lifecycle. The actual HTTP server implementation lives in
`gglib-proxy`; this module provides the runtime integration layer.

# Architecture

- **ProxySupervisor**: Owns proxy state internally, provides start/stop/status
- **gglib-proxy**: HTTP server with OpenAI-compatible endpoints
- Adapters (Tauri, Axum, CLI) call supervisor methods without storing handles

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`models.rs`](models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-coverage.json) |
| [`supervisor.rs`](supervisor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-coverage.json) |
<!-- module-table:end -->

</details>
