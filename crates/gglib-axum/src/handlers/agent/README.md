# agent

<!-- module-docs:start -->

POST /api/agent/chat — server-side agentic loop with SSE streaming.

`compose::prepare` resolves the upstream, validates the request, applies the
configured limits and calls [`compose_agent_loop`](gglib_runtime::compose_agent_loop),
so every caller of the loop does those alike. The limits are resolved by
core's `TurnLimits::resolve`, which the CLI's `chat` and `q` call too: the
iteration limit the request names (for a device's turn, the one its chat's
settings name), then this machine's stored `max_tool_iterations`, then the
default; and the stored `max_stagnation_steps`, which no request names. So a
client that sends no iteration limit, as the chat page and a paired device
do, runs with the stored one. The handler spawns the loop as
a background task and bridges the resulting `mpsc::Receiver<AgentEvent>` to an
Axum [`Sse`] response, each event framed by `compose::frame`.

Inline `<think>` reclassification is handled upstream by
[`gglib_core::normalize::NormalizingStream`] in the LLM adapter, so this
handler only forwards already-typed [`AgentEvent`](gglib_core::domain::agent::AgentEvent)s.

An agent run stamps each turn's `turn_usage` event with the model it drove
(`gglib_app_services::transcript::MadeBy`: locally the model on the port and
its catalogue quantisation, on the paired machine the name and quantisation
that machine has for the model) before logging it, so the
frame a page draws and the row the reply is saved as say the same. A local
run also stamps the context its model was launched with
(`remote_upstream::hold_model`): read once the run holds the model, so it
cannot be an earlier launch's, and only when the primary slot's model is the
run's own by port and id. A run on the paired machine's model, or on a model
in the second slot, stamps none, and the figure is left out: never a default.
The loop itself counts the messages it left out of each request and reports
why the model stopped.

# Which upstream

`remote_upstream` decides, before anything else, whether the loop drives a
llama-server this daemon started (the request's `port`, validated against
the servers it owns, the model resolved against this catalog) or a model of
the machine on the other end of the remote tunnel (`far`, that machine and
the model's id there: the port the tunnel bound, the key from the pairing as
the bearer, and no shaping, because the far proxy runs its own pipeline). A
local run whose messages, history included, carry an image is refused `400
model_cannot_read_images` when the model its server serves has no projector
(`image_gate`); a far run is not judged here, since the far proxy refuses it
by the same code. A
`far` naming this machine is a `400`. Not connected, connected without a key,
or connected to another machine than the ref's is a `409`
(`RemoteOps::far_for`), the last because that machine's same id is another
model.

On the far path the id is looked up there first (`FarProxy::lookup`), so a
model that machine does not have is its `404` before any turn starts; the
turns are sent with the id as the model, so no other model of its name
answers, and the run is counted under, and its turns made by, the name and
quantisation that machine has for it. Locally an absent `model` is the
ordinary case and means "whatever llama-server loaded".

# Cancellation

When the HTTP client disconnects (browser tab closed, `curl` killed, etc.),
Axum drops the SSE response and therefore the [`guard::AgentTaskGuard`] stream
wrapper. Its [`Drop`] impl calls [`tokio::task::JoinHandle::abort`], which cancels the
spawned `AgentLoop` task at its next `await` point — immediately stopping
LLM token generation and any in-flight tool calls without leaking compute
or resources. An agent run (`run.rs`, `PUT /api/runs/{id}?kind=agent`)
runs the same prepared loop detached from any response, so only cancel or
shutdown stops it, and saves the transcript to the request's conversation,
through `gglib_app_services::transcript`, the one writer of a turn's rows,
which the CLI's chat saves through too.
Before it takes a slot, its messages' images, history included, are checked
by their sizes alone (`AttachmentService::check_request`): an id not stored
is `400 attachment_not_found`, and images over 16 MiB together `400
request_images_too_large`, on this machine's model or the paired machine's,
whose images are read here too.
A local run holds its model until it ends (`remote_upstream::hold`), so no
proxy request swaps it out or recycles it mid-run, and the proxy's stall
watchdog waits for the run to end; a person's stop does not. A llama-server
that goes silent mid-reply for five minutes, the proxy's own idle bound, ends
the run with an error (one that never sends headers ends it at the ten-minute
send timeout), so a silent server cannot keep the hold. `launch` reserves the id
in the caller's scope, refuses a run on another machine than the one its
conversation ran on (a `409`: a chat's machine is fixed for its life, and a
paired machine is told apart by its fingerprint), saves the user's message,
names the run's model on the
conversation by its machine (a local run's registry id, a far run's id on the
paired machine, and its name in the settings) so the chat's next turn from
either door runs on it, or is refused for being the paired machine's, has
the conversation remember the Thinking choice its turn said, and starts the
loop.

# The Thinking choice

Core's `gglib_core::domain::thinking::settle` is the one rule for it, read at
every door a turn comes through: a device's turn (`hub_turn::plan`), which is
also how the page's turn on a paired machine arrives there, and this
machine's own run (`run::plan`). The CLI reads a chat it resumes by the same
rule. A turn says `off`, `default` or nothing. `off` runs it with a
thinking budget of `0` and the conversation remembers; `default` runs it with
the request's own `reasoning_budget_tokens` and the conversation forgets;
nothing runs it as the conversation remembers, and a remembered `off` beats
the request's own budget, which a device's turn never has and the page's run
carries once one is set in its Tools popover. A request's own budget and
effort level are never remembered. What is remembered is
`ConversationSettings.thinking`,
written by `launch` beside the run's model, so a refused run, or a repeated
id that starts nothing, writes nothing, and a run with no conversation
applies what it said and remembers nothing.

# A paired device's turn

`hub_turn` is the daemon's `AgentRunStarter`, handed to every proxy it
starts: `PUT /v1/runs/{id}?kind=agent` on the proxy's door carries only a
chat's id and the device's message, with any image named by the id its
upload answered, and the chat's Thinking choice on the turn that changes it;
a turn may be its images alone. An image the turn or the
history names that is not stored is `400 attachment_not_found`, images over
16 MiB together `400 request_images_too_large`, and an image for a model
with no projector `400 model_cannot_read_images`, all before the model is
loaded. The history is rebuilt from the hub's
record as a resumed CLI chat reads it (core's `saved_history`: the system
prompt, then every row but a system one; then the message), the limits from the
conversation's settings (an iteration limit they do not name is this
machine's stored one), no tools unless `enable --allow-mcp` opened the
tunnel to them (then only those the settings name), and the reply runs on the chat's model
(`hub_model`: its own, its settings', its last reply's, the one running on
the hub (the one started last, of several), or the hub's default), loaded first when it is not running, as an agent run in the
device's scope saved to the chat.

<!-- module-docs:end -->
