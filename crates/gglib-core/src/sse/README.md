# sse

<!-- module-docs:start -->

Server-Sent Events (SSE) codec for `OpenAI`-compatible chat completion
streams.

This module reads the `OpenAI` `chat.completion.chunk` SSE wire format
into the typed [`crate::LlmStreamEvent`] domain values, and writes the few
frames gglib adds to a reply. It contains four pieces:

| Submodule | Role |
|-----------|------|
| [`frames`] | Byte-stream → complete lines, and each event: its `data:` payload, with its `id:` and `event:` for a caller that asks (under a size limit, or none); a character split across two chunks is decoded whole |
| [`parser`] | One `data:` payload → its JSON → typed events (`events_of`) |
| [`decoder`] | Stateful byte-stream → `SseItem`s (each comment, each frame with its text, JSON and events, and `[DONE]`), or events alone, one complete line from [`frames`] at a time |
| [`encoder`] | gglib's own frames: a notice as text, a repair's tool calls, a finish, an error frame |

Promoting the codec to `gglib-core` lets every adapter (runtime, proxy,
future GUIs) share a single, well-tested implementation rather than
re-rolling SSE parsing per surface.

<!-- module-docs:end -->
