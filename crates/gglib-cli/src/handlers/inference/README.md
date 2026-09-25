# inference

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-inference-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-inference-complexity.json)

<!-- module-docs:start -->

Inference command handlers.

Handles `serve`, `proxy`, `chat`, and `question` — the top-level commands
that run models. `serve` and `proxy` are the pinned and unpinned modes of
one `POST /api/proxy/start` call on the daemon — they differ in
`StartProxyBody::pinned` and little else, so their handlers mirror each
other. Shared inference-config resolution and logging live in the
[`shared`] submodule to avoid duplication.

<!-- module-docs:end -->
