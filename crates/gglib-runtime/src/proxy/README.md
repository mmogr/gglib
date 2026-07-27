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

# One entry point, two modes

`start_proxy_standalone` backs both CLI commands. `StandaloneProxyParams::pinned`
is the only difference between them:

| | `gglib proxy` | `gglib serve <model>` |
|---|---|---|
| `pinned` | `None` — auto-swap on request | `Some(PinnedModel)` — refuse others |
| `/v1/models` | the whole catalog | the pinned model only |

Everything else — the Axum layer, cache lifecycle, dashboard, SSE, MCP gateway,
council wiring and shutdown — is shared verbatim. `serve` is a *mode* of the
proxy, not a second stack.

The catalog row follows from the first: a model the proxy would refuse should
never be advertised, or a client that cannot switch models picks one and gets
`PinnedModelMismatch` for something it was offered. Profile variants of the
pinned model and the council virtuals are still listed — neither changes which
model actually runs, so neither can trip the guard.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`banner.rs`](banner.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-banner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-banner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-banner-coverage.json) |
| [`models.rs`](models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-models-coverage.json) |
| [`params.rs`](params.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-params-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-params-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-params-coverage.json) |
| [`supervisor.rs`](supervisor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-supervisor-coverage.json) |
<!-- module-table:end -->

</details>
