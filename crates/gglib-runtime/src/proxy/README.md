# proxy

<!-- module-docs:start -->

OpenAI-compatible proxy module.

This module provides the proxy supervisor for managing the OpenAI-compatible
proxy server lifecycle. The actual HTTP server implementation lives in
`gglib-proxy`; this module provides the runtime integration layer.

# Architecture

- **`ProxySupervisor`**: Owns proxy state internally, provides start/stop/status
- **gglib-proxy**: HTTP server with OpenAI-compatible endpoints
- Adapters (Tauri, Axum, CLI) call supervisor methods without storing handles

# One entry point, two modes

`ProxySupervisor::start` backs both CLI commands, and it takes no position on
either: `ProxyConfig` has no pinning field. Pinning is a *residency* concern,
applied through `ProcessManager::set_pin`, and it reaches the daemon's shared
runtime as the `pinned` field of `POST /api/proxy/start`. The two commands
differ only in whether they send one:

| | `gglib proxy` | `gglib serve <model>` |
|---|---|---|
| `pinned` | `None` — auto-swap on request | `Some(PinnedSpec)` — refuse others |
| `/v1/models` | the whole catalog | the pinned model only |

Everything else — the Axum layer, cache lifecycle, dashboard, SSE, MCP gateway
and shutdown — is shared verbatim. `serve` is a *mode* of the
proxy, not a second stack.

The catalog row follows from the first: a model the proxy would refuse should
never be advertised, or a client that cannot switch models picks one and gets
`PinnedModelMismatch` for something it was offered. Profile variants of the
pinned model are still listed — a profile does not change which model actually
runs, so it cannot trip the guard.

<!-- module-docs:end -->
