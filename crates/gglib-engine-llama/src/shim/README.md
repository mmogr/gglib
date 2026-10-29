# shim

<!-- module-docs:start -->

The server gglib-proxy forwards to in place of llama-server: axum on a free
port of 127.0.0.1, answering in llama-server's shape at the pin
(`contracts/upstream/llama/shape.txt`, `bodies/`, and
`tools/server/server-task.cpp` at the pinned tag).

A chat request is checked for what the engine refuses, read and rendered
through the model's template, its generation prompt found to say where the
reply starts, then waits its turn: one generation runs at a time, as one
llama-server slot serves. The generation runs on a thread of its own,
tokenizes the prompt and refuses it when it leaves no token of the context
free (an empty one is answered with nothing, ended at its length, as
llama-server answers it), then hands its events over a bounded channel. Its first event decides
the answer, as llama-server's first result does: an error is answered alone,
as JSON, streamed or not. Otherwise the events become chunks or one
completion. A stop string found in the reply cuts it there and raises the
generation's abort; so does the reply being dropped, which is how a client
that hangs up ends its generation within one decode step.

The reply is split as llama-server splits it under its default
`--reasoning-format deepseek`: a `<think>` block at its start, or one the
generation prompt opened, goes out as `reasoning_content`, the rest as
`content`, and neither carries a tag. gglib-proxy then reads a phone's
stream as it reads a desktop llama-server's. Paths the shim does not serve
get llama-server's 404, and a slot's save or restore its 501.

# Structure

- `mod.rs` - [`start`], [`Shim`], and the routes
- `chat.rs` - `POST /v1/chat/completions`: the refusals, the turn, the
  decode thread, and the reply streamed or whole
- `reply.rs` - a generation's events turned into chunks: the role chunk,
  progress, reasoning and answer, the finish, usage and timings
- `reasoning.rs` - where a reply starts, from its generation prompt, and
  the reply split into reasoning and answer as it streams
- `frames.rs` - llama-server's JSON: the chunk envelope, the completion,
  `usage`, `timings`, and its errors
- `stop.rs` - `stop` strings found as the reply streams
- `info.rs` - `GET /props`, `GET /health`, `GET /slots` and
  `POST /slots/{id}` (501), `POST /apply-template`, and the 404
- `test_support.rs` - a shim over a `ScriptedDecoder`, for the tests below
- `stream_tests.rs` - gglib-core's SSE decoder reading the stream, the
  unary answer, and a client that hangs up
- `shape_tests.rs` - every answer's key paths and types against the pin's
  `shape.txt`
- `refusal_tests.rs` - what is refused, and how, and the empty prompt
  answered with nothing
- `template_tests.rs` - `POST /apply-template` and the decoded prompt
  against the pin's render
- `reasoning_tests.rs` - the split piece by piece, and where each pinned
  template's reply starts
- `thinking_tests.rs` - a thinking model's reply over the wire, streamed
  and whole, and a follow-up turn's cache

<!-- module-docs:end -->
