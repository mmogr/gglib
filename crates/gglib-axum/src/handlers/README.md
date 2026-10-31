# handlers

<!-- module-docs:start -->

HTTP request handlers for the Axum web server.

Handlers are organized into domain-scoped subdirectories:
- [`model`]  — CRUD, verification, downloads, `HuggingFace` discovery
- [`config`] — settings, system setup
- [`agent`]  — `POST /api/agent/chat`, the server-side agentic loop, and the
  agent runs the daemon and a paired device start
- [`benchmark`] — compare, perf, tune and agentic runs, and the run history
- [`proxy`]  — the OpenAI-compatible proxy's status, start, stop and loop-guard log
- [`remote`] — the remote tunnel, both sides, and the far machine's chats,
  runs, images and models
- [`runs`]   — `/api/runs/*`, this machine's door to the daemon's runs

`attachments.rs` is `POST /api/attachments` and `GET /api/attachments/{id}`:
an image the chat page sends once, as the raw body, and names by its id in
every message after. Both are thin over core's `AttachmentService`.

`generation.rs` is `GET /api/generation/turn`: an LLM turn on the daemon's
generation gate, held for as long as the connection stays open. It streams
`waiting` while a render is in the way, then `granted`, then only
keep-alives until the client goes, or `refused` and the end. `gglib chat`
opens it around each send, since its replies come from a llama-server port
past the proxy.

`images.rs` is `POST /api/images/generations`: the proxy's
`/v1/images/generations` handler body, mounted at the daemon's door over the
same image driver, so `gglib image` draws when the proxy is stopped or
keyed. `GET /api/images/drawing` (query `far`, `calls_tools`, `image_model`)
says whether a message sent with Draw pressed can draw here, and why not,
lists this machine's image models, and with `image_model=<id>` judges that
one as a request naming it would be; `generate_image`
is never in `/api/builtin/tools`, whose list the page lets a person switch
tools on from, and neither are the file tools, which need the run's folder.

`secrets.rs` is `POST /api/secrets/reveal`: a stored secret, for the page's
Show and Copy and the proxy key its dashboard, tray and Clear Cache present,
from core's `SecretsService::reveal` (ADR 0027). The daemon token gets any
secret but the pairing key, which only the CLI prints on this machine. A
shared daemon's API key, which clients on the network hold, gets back only
itself, which the page opened over the network presents to the proxy.

`chat_title.rs` is `POST /api/chat`: a chat's title, asked of the
llama-server the chat runs on. The body names the port, the messages, a
temperature and a token cap, and any other key is refused. The request runs
core's `request_pipeline` with thinking off, is never streamed, and is
answered with the model's text.

<!-- module-docs:end -->
