# sse

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-sse-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-sse-complexity.json)

<!-- module-docs:start -->

Server-Sent Events (SSE) codec for `OpenAI`-compatible chat completion
streams.

This module is the **single source of truth** for translating between
the `OpenAI` `chat.completion.chunk` SSE wire format and the typed
[`crate::LlmStreamEvent`] domain values.  It contains three pieces:

| Submodule | Role |
|-----------|------|
| [`parser`] | Parse one `data:` JSON payload → typed events |
| [`decoder`] | Stateful byte-stream → events (line buffering, `[DONE]`) |
| [`encoder`] | Typed event → `data:` JSON payload (for re-emission) |

Promoting the codec to `gglib-core` lets every adapter (runtime, proxy,
future GUIs) share a single, well-tested implementation rather than
re-rolling SSE parsing per surface.

<!-- module-docs:end -->
