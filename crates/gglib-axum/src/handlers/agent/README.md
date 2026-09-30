# agent

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-agent-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-agent-complexity.json)

<!-- module-docs:start -->

POST /api/agent/chat — server-side agentic loop with SSE streaming.

`compose::prepare` resolves the upstream, validates the request, applies the
configured limits and calls [`compose_agent_loop`](gglib_runtime::compose_agent_loop),
so every caller of the loop does those alike. The handler spawns the loop as
a background task and bridges the resulting `mpsc::Receiver<AgentEvent>` to an
Axum [`Sse`] response, each event framed by `compose::frame`.

Inline `<think>` reclassification is handled upstream by
[`gglib_core::normalize::NormalizingStream`] in the LLM adapter, so this
handler only forwards already-typed [`AgentEvent`](gglib_core::domain::agent::AgentEvent)s.

An agent run stamps each turn's `turn_usage` event with the model it drove
(`compose::MadeBy`: locally the model on the port and its catalogue
quantisation, remotely the name the request gave) before logging it, so the
frame a page draws and the row the reply is saved as say the same.

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
or resources. An agent run (`run.rs`, `PUT /api/runs/{id}?kind=agent`)
runs the same prepared loop detached from any response, so only cancel or
shutdown stops it, and saves the transcript to the request's conversation.
A local run holds its model until it ends (`remote_upstream::hold`), so no
proxy request swaps it out or recycles it mid-run. `launch` reserves the id
in the caller's scope, saves the user's message and starts the loop.

# A paired device's turn

`hub_turn` is the daemon's `AgentRunStarter`, handed to every proxy it
starts: `PUT /v1/runs/{id}?kind=agent` on the proxy's door carries only a
chat's id and the device's message. The history is rebuilt from the hub's
record (the system prompt, every row, the message), the limits from the
conversation's settings, no tools unless `enable --allow-mcp` opened the
tunnel to them (then only those the settings name), and the reply runs on the chat's model
(`hub_model`: its own, its settings', its last reply's, or the hub's
default), loaded first when it is not running, as an agent run in the
device's scope saved to the chat.

<!-- module-docs:end -->
