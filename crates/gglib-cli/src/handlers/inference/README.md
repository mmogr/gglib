# inference

<!-- module-docs:start -->

Inference command handlers.

Handles `serve`, `proxy`, `chat`, and `question` — the top-level commands
that run models. `serve` and `proxy` are the pinned and unpinned modes of
one `POST /api/proxy/start` call on the daemon — they differ in
`StartProxyBody::pinned` and little else, so their handlers mirror each
other. What `serve` says on stderr of its launch lives in the [`shared`]
submodule. An image model takes `serve_image` instead: it ensures
stable-diffusion.cpp rather than llama.cpp, refuses the chat-only flags by
name, and loads the model once through the pinned proxy. `chat` and `question` resolve no sampling here: they hand their
flags and the stored layers to the agent loop's adapter, whose request
pipeline folds them (`agent_chat::config`).

<!-- module-docs:end -->
