# proxy

<!-- module-docs:start -->

OpenAI-compatible proxy supervisor.

This module provides the proxy supervisor for managing the OpenAI-compatible proxy server lifecycle. The actual HTTP server implementation lives in `gglib-proxy`; this module provides the runtime integration layer.

## Architecture

```text
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│ Adapters        │     │ ProxySupervisor │     │   gglib-proxy   │
│ (Tauri, Axum,   │───▶│ - start/stop    │───▶│ OpenAI-compat   │
│  CLI)           │     │ - status        │     │ HTTP server     │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

## Key Types

| Type | Description |
|------|-------------|
| `ProxySupervisor` | Owns proxy state, provides start/stop/status |
| `ProxyConfig` | Configuration (host, port, llama base port) |
| `ProxyStatus` | Runtime status (starting, running, stopped) |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`models.rs`](models) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-coverage.json) |
| [`supervisor.rs`](supervisor) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-coverage.json) |
<!-- module-table:end -->

</details>
