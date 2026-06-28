# agent

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-agent-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-agent-complexity.json)

<!-- module-docs:start -->

POST /api/agent/chat — server-side agentic loop with SSE streaming.

The handler calls [`compose_agent_loop`] to wire up the LLM adapter, MCP
tool executor, and agent loop, spawns the loop as a background task, and
bridges the resulting `mpsc::Receiver<AgentEvent>` to an Axum [`Sse`]
response.

Inline `<think>` reclassification is handled upstream by
[`gglib_core::normalize::NormalizingStream`] in the LLM adapter, so this
handler only forwards already-typed [`AgentEvent`]s.

# Cancellation

When the HTTP client disconnects (browser tab closed, `curl` killed, etc.),
Axum drops the SSE response and therefore the [`guard::AgentTaskGuard`] stream
wrapper. Its [`Drop`] impl calls [`JoinHandle::abort`], which cancels the
spawned `AgentLoop` task at its next `await` point — immediately stopping
LLM token generation and any in-flight tool calls without leaking compute
or resources.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`dto.rs`](dto.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-dto-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-dto-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-dto-coverage.json) |
| [`guard.rs`](guard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-guard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-guard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-guard-coverage.json) |
<!-- module-table:end -->

</details>
