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

# Which upstream

`remote_upstream` decides, before anything else, whether the loop drives a
llama-server this daemon started (the request's `port`, validated against
the servers it owns, the model resolved against this catalog) or the machine
on the other end of the remote tunnel (`"remote": true`: the port the tunnel
bound, the key from the pairing as the bearer, and no shaping, because the
far proxy runs its own pipeline). Not connected, or connected without a key,
is a `409` that names `gglib remote join`.

It also settles the request's `model`, because the two paths mean opposite
things by an absent one. Locally an absence is the ordinary case and means
"whatever llama-server loaded". Remotely it is a `400`: there is no catalog
here for the far machine's names and this machine's default is not
substituted, so an unnamed model would reach that machine as `""` and come
back `404 Model '' not found` — a real answer through a working tunnel.

# Cancellation

When the HTTP client disconnects (browser tab closed, `curl` killed, etc.),
Axum drops the SSE response and therefore the [`guard::AgentTaskGuard`] stream
wrapper. Its [`Drop`] impl calls [`tokio::task::JoinHandle::abort`], which cancels the
spawned `AgentLoop` task at its next `await` point — immediately stopping
LLM token generation and any in-flight tool calls without leaking compute
or resources.

<!-- module-docs:end -->
