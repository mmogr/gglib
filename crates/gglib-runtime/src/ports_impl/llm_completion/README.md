# LLM Completion

<!-- module-docs:start -->

Concrete [`LlmCompletionPort`] adapter for a llama-server instance.

Translates domain [`AgentMessage`] / [`ToolDefinition`] values into the
OpenAI-compatible JSON wire format, POSTs to
`{base_url}/v1/chat/completions` with `"stream": true`, and maps the
response SSE frames back to [`LlmStreamEvent`] values.

The `base_url` is the server root without a trailing path component,
e.g. `"http://127.0.0.1:9000"`.  This allows the adapter to target any
reachable host (Docker networks, remote servers, CI environments).

# Authentication

The adapter sends nothing by default: a llama-server on loopback asks for
nothing. [`LlmCompletionAdapter::with_far_machine`] adds
`Authorization: Bearer …` to every request, for the one upstream that demands
it — the remote tunnel's loopback port, which is another machine's proxy
(ADR 0012) and whose listener deliberately injects no credential of its own.
The token is held in a field the struct never prints; neither it nor
[`FarMachine`] derives `Debug`.

The key does not travel alone: it arrives in a [`FarMachine`], beside the
name of the machine that issued it. Only the surface that resolved the
remote upstream knows whose port it just chose, and the adapter it hands the
result to holds a URL that is loopback either way — so the identity comes
down with the key, and the send loop can name the machine when that machine
turns the key away. See `retry/` for what it then says, and for why nothing
may key on the refusal code alone.

# Images

A user message names its images by id. Just before a request is sent, each
is read from the [`AttachmentStore`] given by
[`LlmCompletionAdapter::with_attachments`] and the message's `content`
becomes an array: the text part, when there is text, then one `image_url`
part an image. A message with no image is sent as the bare string it always
was. An id the store lacks, or images over 16 MiB together, end the request
before anything is sent.

[`AttachmentStore`]: gglib_core::ports::AttachmentStore

# Sampling

The adapter resolves no sampling of its own. A caller hands over what a
person chose for the turn ([`LlmCompletionAdapter::with_sampling`]: flags
typed at a terminal, the reasoning controls a run's request names) and,
apart from it, the stored layers beneath ([`LlmCompletionAdapter::with_layers`]:
the selected profile and the settings' global defaults). With those layers
goes whether a turn with tools gets the agentic temperature ceiling: a chat
on one of this machine's models hands over the settings' switch, and a caller
with no settings to follow turns it on, as an adapter handed no layers has
it. The model's own values arrive in its context.
`gglib_core::request_pipeline::apply` folds them once, as each request is
shaped, which is what lets it tell a temperature a person chose from one
nobody did. A caller that folds a layer into the first argument defeats that:
the value arrives as a choice. What a request carries is what that fold
resolved: a parameter the caller named and the fold passed over (a penalty
without the temperature it travels with, beneath a layer that names one) is
taken back out before the request is sent.
One caller asks for it to be left in
([`LlmCompletionAdapter::with_passed_over_kept`]): a tune sweep, whose
candidate is what the sweep measures and not a flag for the ladder to judge.
[`LlmCompletionAdapter::with_sampling_observer`] tells a caller how a request
resolved, so none has a reason to fold a ladder to find out.

# Layout

`mod.rs` holds the struct and its request path; `builder.rs` the two
constructors and the `with_*` builders; `far_machine.rs` the other end of a
remote turn; `retry/` the send loop; `body.rs` and `stream.rs` the two ends
of the wire format; `images.rs` reads the images a message names by id and
writes each as an `image_url` data URL, the one place an id becomes bytes; `writing_time.rs` times the model's writing on the
decoded stream, before a dialect parser holds tool-call markup back.

# Lifetime

Prefer constructing one adapter **per request** via
[`LlmCompletionAdapter::with_client`] and passing a clone of the
application-level `reqwest::Client` (stored in `AppState`) so all requests
share a single connection pool.  The `new` constructor is still available
for standalone use (e.g. CLI) and allocates its own pool.

```ignore
let adapter = LlmCompletionAdapter::new("http://127.0.0.1:9000", None::<String>);
let agent   = AgentLoop::build(Arc::new(adapter), tool_executor, None);
```

<!-- module-docs:end -->
