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
nothing. [`LlmCompletionAdapter::with_door`] adds `Authorization: Bearer …` to
every request, for an upstream that is a gglib endpoint: this machine's own,
which a chat turn dials as any client does, or the remote tunnel's loopback
port, which is another machine's proxy (ADR 0012) and whose listener
deliberately injects no credential of its own. The token is held in a
field the struct never prints: the adapter derives no `Debug`, and
[`Door`]'s `Debug` leaves the key out.

The key does not travel alone: it arrives in a [`Door`], beside the name of
the machine that issued it (`None` for this machine's own endpoint). Only the
surface that chose the endpoint knows whose it is, and the adapter it hands
the result to holds a URL that is loopback either way — so the identity comes
down with the key, and the send loop can say in the door's words when the
endpoint turns the key away. See `retry/` for what it then says, and for why
nothing may key on the refusal code alone.

# Through a door

A send through a door is proxied: the endpoint runs the request pipeline
over it as it does over any client's request, so the adapter does not. It
folds no sampling ladder (the person's own choice, written into the body,
reaches the endpoint as a client's: thinking off is
`reasoning_budget_tokens: 0`), merges no turns, trims nothing and writes no
grammar; the first call's `tool_choice` goes as it was written. The reply
comes back with the dialect's markup already parsed into `OpenAI` tool
calls, so no dialect parser runs over it. The images a message names are
still read here, since the endpoint takes them inline.

# Images

A user message names its images by id. Just before a request is sent, each
is read from the [`AttachmentStore`] given by
[`LlmCompletionAdapter::with_attachments`] and the message's `content`
becomes an array: the text part, when there is text, then one `image_url`
part an image. A message with no image is sent as the bare string it always
was. An id the store lacks, or images over 16 MiB together, end the request
before anything is sent.

A tool message is sent as its text; the ids of the images its tool made are
not. One of them is read all the same, for a model whose
`ModelContext::image_input` is set: the most recent picture the drawing
tool made, written at the start of the first user message after it as a
text part ([`DRAWN_PICTURE_LINE`]) and an `image_url` part
([`shown_picture`] is the rule). It is counted with the request's images
and never ends a request: a picture that is not stored, cannot be read or
would take the request over 16 MiB is left out, with its line.

[`DRAWN_PICTURE_LINE`]: gglib_core::request_pipeline::DRAWN_PICTURE_LINE
[`shown_picture`]: gglib_core::request_pipeline::shown_picture

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
the value arrives as a choice. The caller's own parameters sit above every
layer it hands over, and the coupling rule passes over only layers beneath
the one that claims the temperature, so the fold never passes one of them
over (ADR 0022).

# A turn's limit

llama-server sets no limit of its own on a reply (`n_predict: -1`), so a turn
the model never ends runs until the context is full. The agent loop's adapter
([`LlmCompletionAdapter::with_turn_limit`], set by `compose_agent_loop`) gives a request that leaves the ladder
with no `max_tokens` a limit of 16384 tokens, 32768 for a `reasoning`-tagged
model, or a positive `reasoning_budget_tokens` plus 8192 when that is more. A
request that names one keeps it: `--max-tokens`, a stored setting, the
`reasoning` recipe's 32768. A reply cut off at the limit while it writes a
call, or before it writes any text, ends the loop's run; one cut off in its
text is taken as if it had stopped. The words after a Draw's picture are the
exception and are always answered for. The limit is written after the ladder is
folded, so the sampling observer, and `gglib model explain`, do not report it.
A send to another machine is not limited: a run on a paired machine's model
gets no limit from this side, and that machine's proxy applies its own
settings for the model, as it would for any client that names none. A send
through this machine's endpoint is: the endpoint takes `max_tokens` as the
client's own budget whatever it trusts, so a limit written here would
outrank one its ladder names. The ladder is folded on a copy, with the
layers and model the endpoint folds with, and the limit is written only when
that fold names no `max_tokens`. An adapter without it, such as a benchmark
arm's, sends none.

# Layout

`mod.rs` holds the struct and its request path; `builder.rs` the two
constructors and the `with_*` builders; `door.rs` the endpoint a turn is
sent through; `retry/` the send loop; `body.rs` and `stream.rs` the two ends
of the wire format; `images.rs` reads the images a message names by id and
writes each as an `image_url` data URL, the one place an id becomes bytes; `writing_time.rs` times the model's writing on the
decoded stream, before a dialect parser holds tool-call markup back;
`turn_limit.rs` gives a loop's turn its limit.

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
