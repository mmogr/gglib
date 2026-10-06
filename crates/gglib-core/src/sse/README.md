# sse

<!-- module-docs:start -->

Server-Sent Events (SSE) codec for `OpenAI`-compatible chat completion
streams.

This module is the **single source of truth** for translating between
the `OpenAI` `chat.completion.chunk` SSE wire format and the typed
[`crate::LlmStreamEvent`] domain values.  It contains four pieces:

| Submodule | Role |
|-----------|------|
| [`frames`] | Byte-stream → complete lines, and each event: its `data:` payload, with its `id:` and `event:` for a caller that asks (under a size limit, or none); a character split across two chunks is decoded whole |
| [`parser`] | Parse one `data:` JSON payload → typed events |
| [`decoder`] | Stateful byte-stream → events (`[DONE]`), one complete line from [`frames`] at a time |
| [`encoder`] | Typed event → `data:` JSON payload (for re-emission); the usage frame also carries a context reading when the encoder is given one |

Promoting the codec to `gglib-core` lets every adapter (runtime, proxy,
future GUIs) share a single, well-tested implementation rather than
re-rolling SSE parsing per surface.

<!-- module-docs:end -->
