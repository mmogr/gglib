# inference

<!-- module-docs:start -->

Inference command handlers.

Handles `serve`, `proxy`, `chat`, and `q` — the top-level commands
that run models. `serve` and `proxy` are the pinned and unpinned modes of
one `POST /api/proxy/start` call on the daemon — they differ in
`StartProxyBody::pinned` and little else, so their handlers mirror each
other. What `serve` says on stderr of its launch lives in the [`shared`]
submodule. An image model takes `serve_image` instead: it ensures
stable-diffusion.cpp rather than llama.cpp, refuses the chat-only flags by
name, and loads the model once through the pinned proxy. `chat` and
`q` are clients of the daemon's door: they save their flags with the
chat and send each turn to the daemon, which runs it (`agent_chat::turn`);
they resolve no sampling and run no loop here.

`switch` is what `proxy`, `serve` and `up` do when the daemon says the
endpoint is on otherwise than they ask (`endpoint_differs`): ask, in a
terminal, in the daemon's words for what runs and what switching does; take
`-y` as a yes; refuse with one sentence naming `-y` when nobody is there to
ask; and on a yes send the same start with `replace`, which the daemon
carries out as one operation.

<!-- module-docs:end -->
