# gglib-axum

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json)

Layer: **adapter** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

HTTP API server for gglib — provides REST endpoints for the web UI and external integrations.

It also hosts the daemon: the single process that owns llama-server and holds
the exclusive lock every other surface connects through. When the CLI, the
desktop app, or the web UI needs the runtime, this is what they talk to.

## Architecture

This crate is in the **Adapter Layer** — it exposes gglib functionality via HTTP using the Axum framework.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-axum                                           │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │   daemon/   │ ──► │ bootstrap.rs│                                                │
│  │ run_daemon: │     │  DI setup   │                                                │
│  │ lock, serve │     │  & wiring   │                                                │
│  └──────┬──────┘     └─────────────┘                                                │
│         │                                                                           │
│         │            ┌─────────────┐     ┌─────────────┐                            │
│         └──────────► │  routes.rs  │ ──► │  handlers/  │                            │
│                      │   Router    │     │ one folder  │                            │
│                      │  mounting   │     │  or file    │                            │
│                      └─────────────┘     │ per domain  │                            │
│                                          └─────────────┘                            │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │    dto/     │     │  error.rs   │                                                │
│  │  Request &  │     │  HTTP error │                                                │
│  │  Response   │     │  handling   │                                                │
│  └─────────────┘     └─────────────┘                                                │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Module Descriptions:**
- **`daemon/`** — `run_daemon`, which `gglib daemon run` calls: the one process per machine that owns llama-server, and its singleton lock
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`config.rs`** — `ServerConfig`: what the server is given
- **`chat_api.rs`** — Conversations and their messages
- **`chat_changes.rs`** — A chat read with its branch points, and a change made to it (ADR 0017)
- **`error.rs`** — HTTP error types and JSON error responses
- **`routes.rs`** — Route definitions and handler mounting
- **`routes_remote.rs`**, **`routes_runs.rs`**, **`routes_benchmark.rs`** — The routes nested under `/api/remote`, `/api/runs` and `/api/benchmark`
- **`access.rs`** — `DaemonAccess`: the daemon's Host allowlist, its token, and the error shape a refusal is written in; the Host and origin guards that apply it are the proxy's
- **`trust.rs`** — The bearer guard on `/api`: the daemon token, and on a `--share-lan` daemon its API key besides
- **`state.rs`** — `AppState`, the state every handler and router shares
- **`proxy_watch.rs`** — Turns the proxy supervisor's exit channel into a `ProxyCrashed` event on the event stream
- **`sse.rs`** — Server-Sent Events utilities for streaming
- **`ui.rs`** — The dashboard, compiled into the binary, and its HTTP contract
- **`dto/`** — Request/response DTOs for API endpoints
- **`handlers/model/`** — Model CRUD, verification, downloads, `HuggingFace` discovery handlers
- **`handlers/config/`** — Settings, system setup and engine handlers
- **`handlers/agent/`** — `POST /api/agent/chat`, the server-side agentic loop over SSE, and the agent runs started at `/api/runs` and by a paired device
- **`handlers/runs/`** — `/api/runs/*`: start, list, read and cancel the daemon's runs
- **`handlers/remote/`** — The remote tunnel: enable, disable, status, invite and the devices on the serve side; join, disconnect and kill on the connect side; and the far machine's chats, runs, images and models
- **`handlers/proxy/`** — Proxy status, start, start-pinned and stop, and the loop guard's log
- **`handlers/benchmark/`** — Compare, perf, tune and agentic runs over SSE, the tune apply, and the run history
- **`handlers/chat_title.rs`** — A chat's title, asked of the model the chat runs on
- **`handlers/attachments.rs`**, **`images.rs`**, **`generation.rs`**, **`secrets.rs`**, **`servers.rs`**, **`mcp.rs`**, **`builtin.rs`**, **`events.rs`**, **`daemon.rs`**, **`version.rs`** — One file per domain; the endpoints table below says what each answers

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Liveness probe outside `/api`, needing no token: service, status, version, build fingerprint and the debug switches in effect |
| `GET` | `/api/models` | List models, optionally filtered and sorted by query parameters |
| `POST` | `/api/models` | Add a new model from a local file; a file already in the library is a 409 |
| `GET` | `/api/models/{id}` | One model |
| `PUT` | `/api/models/{id}` | Update a model; `projectorPath` links it to a projector, `null` unlinks it; `components` links an image model's files by role, `null` for a role unlinks it |
| `PATCH` | `/api/models/{id}` | The same partial update as `PUT` |
| `DELETE` | `/api/models/{id}` | Remove a model |
| `PATCH` | `/api/models/{id}/capabilities` | Set or clear single capability flags, touching no other field |
| `POST` | `/api/models/{id}/retag` | Run capability detection again over the model's stored metadata (`gglib model retag`) |
| `GET` | `/api/models/{id}/upgrade-check` | Whether `HuggingFace` has a newer revision of the model (`gglib model check-updates`) |
| `POST` | `/api/models/{id}/upgrade` | Download the latest revision and rewrite the row (`gglib model upgrade`); answers the download's ID, `null` when already at the latest |
| `GET` | `/api/models/{id}/detail` | Every stored field of a model, for the inspector |
| `GET` | `/api/models/{id}/explain` | The resolved sampling parameters and which layer supplied each (`gglib model explain`); `?profile=NAME` for a profile |
| `GET` | `/api/models/{id}/projectors` | The projector files the inspector's picker offers for a model |
| `GET` | `/api/models/{id}/components` | For each role an image model's family needs, the files the inspector's picker offers |
| `GET` | `/api/models/{id}/agentic-history` | Past raw-vs-gglib A/B reports for the model, most recent first |
| `GET` | `/api/models/{id}/tags` | A model's tags |
| `POST` | `/api/models/{id}/tags` | Add a tag, named in the body |
| `POST` | `/api/models/{id}/tags/{tag}` | Add a tag, named in the path |
| `DELETE` | `/api/models/{id}/tags/{tag}` | Remove a tag |
| `GET` | `/api/models/tags` | Every tag in use |
| `GET` | `/api/models/filter-options` | The filter options for the model list |
| `POST` | `/api/models/{id}/verify` | Verify model integrity (streams progress via SSE) |
| `GET` | `/api/models/{id}/updates` | Check for `HuggingFace` updates |
| `POST` | `/api/models/{id}/repair` | Delete a model's unhealthy files and queue the download that fetches them again; answers that download's id and the files |
| `GET` | `/api/models/downloads/queue` | Download queue snapshot |
| `POST` | `/api/models/downloads/queue` | Queue a download; answers `{ "id" }`, the download's ID |
| `POST` | `/api/models/downloads/{id}/cancel` | Cancel a waiting or running download, every file of it |
| `DELETE` | `/api/models/downloads/{id}` | The same; for a download that has ended, drop its finished entry |
| `POST` | `/api/models/downloads/reorder` | Move one download in the queue |
| `POST` | `/api/models/downloads/reorder-full` | Reorder the whole queue |
| `POST` | `/api/models/hf/search` | Search `HuggingFace`; `kind` is `chat` (the default) or `image` |
| `GET` | `/api/models/hf/model/{*model_id}` | One `HuggingFace` repo's summary, by its `owner/repo` id |
| `GET` | `/api/models/hf/quantizations/{model_id}` | The quantizations a repo offers |
| `GET` | `/api/models/hf/tool-support/{model_id}` | Whether a repo's model supports tool calling |
| `GET` | `/api/config/settings` | Get application settings |
| `PUT` | `/api/config/settings` | Update application settings |
| `PATCH` | `/api/config/settings` | The same as `PUT` |
| `POST` | `/api/config/profiles/install-templates` | Add the starter profiles; a stored profile of the same name is kept and named in `kept` |
| `GET` | `/api/config/system/memory` | System memory, or `null` when it cannot be read |
| `GET` | `/api/config/system/models-directory` | The models directory |
| `PUT` | `/api/config/system/models-directory` | Change the models directory |
| `GET` | `/api/config/system/setup-status` | The first-run wizard's setup status |
| `GET` | `/api/config/system/engines/{kind}` | An engine's status, `kind` `llama` or `stable_diffusion`: what is installed, its pin, what an install would download, and the model running on it |
| `POST` | `/api/config/system/engines/{kind}/install` | Install or update the engine's pinned download, progress over SSE; refused while a model runs on it |
| `POST` | `/api/config/system/engines/{kind}/uninstall` | Remove what the install put in the engine's folder; 409 while a model runs on it |
| `POST` | `/api/config/system/setup-python` | Set up the Python fast-download helper |
| `POST` | `/api/config/system/disable-fast-downloads` | Remove the fast-download helper; downloads go back to native HTTP |
| `GET` | `/api/config/system/diagnostics` | Dependencies, resolved paths and acceleration (`gglib config check-deps`, `paths` and `fast-downloads status` in one) |
| `GET` | `/api/config/system/recommend-model` | A model sized to this machine's memory, or `null` when none fits |
| `GET` | `/api/servers` | The running model servers |
| `POST` | `/api/servers/start` | Start llama-server (id in the body) |
| `POST` | `/api/servers/stop` | Stop llama-server (id in the body) |
| `GET` | `/api/servers/{id}/tool-support` | Whether a running server's model supports tool calls, and the format its template uses |
| `GET` | `/api/servers/{port}/logs` | A server's logs so far |
| `GET` | `/api/servers/{port}/logs/stream` | A server's logs as they come (SSE) |
| `GET` | `/api/builtin/tools` | The built-in tools a person can switch on |
| `GET` | `/api/mcp/servers` | List MCP servers |
| `POST` | `/api/mcp/servers` | Add an MCP server |
| `PUT` | `/api/mcp/servers/{id}` | Update an MCP server |
| `DELETE` | `/api/mcp/servers/{id}` | Remove an MCP server |
| `PUT` | `/api/mcp/servers/{id}/remote` | Open an MCP server to paired devices, or close it (`{"allow": bool}`) |
| `POST` | `/api/mcp/servers/{id}/start` | Start MCP server |
| `POST` | `/api/mcp/servers/{id}/stop` | Stop MCP server |
| `POST` | `/api/mcp/servers/{id}/resolve` | Resolve the server's executable path |
| `POST` | `/api/mcp/servers/{id}/test` | Start a throwaway instance, list its tools and stop it (`gglib mcp test`) |
| `POST` | `/api/secrets/reveal` | The stored secret the body names, or `null` (ADR 0027) |
| `GET` | `/api/proxy/status` | The proxy's status |
| `POST` | `/api/proxy/start` | Start the proxy; one already running with the same pin, sampling override and default profile is a success, and a different request is a 409 |
| `POST` | `/api/proxy/start-pinned` | Start the proxy pinned to one model (the GUI's `gglib serve`) |
| `POST` | `/api/proxy/stop` | Stop the proxy; one already stopped is a success |
| `GET` | `/api/proxy/loop-guard-trips` | The loop guard's log, one row per UTC day, model, version and mode, newest day first |
| `POST` | `/api/daemon/shutdown` | Stop the daemon; answers 202 and tears down after the server stops accepting |
| `GET` | `/api/generation/turn` | An LLM turn on the daemon's generation gate, held while the connection is open (`gglib chat`) |
| `GET` | `/api/events` | The event stream (SSE) every client of the daemon reads |
| `POST` | `/api/events` | Put one event on that stream: what a `gglib` command sends for a change it made to the library in its own process |
| `POST` | `/api/agent/chat` | The server-side agentic loop, streamed as SSE |
| `GET` | `/api/runs` | Every run, newest first |
| `GET` | `/api/runs/{id}` | One run |
| `PUT` | `/api/runs/{id}` | Start a chat run with the body as its request, or answer with the run that has the id (201 new, 200 existing); `?kind=agent` runs the agent loop |
| `GET` | `/api/runs/{id}/events` | A run's events (SSE), from `?after=N` |
| `POST` | `/api/runs/{id}/cancel` | Cancel a run; idempotent |
| `GET` | `/api/conversations` | List conversations |
| `POST` | `/api/conversations` | Create a conversation |
| `GET` | `/api/conversations/{id}` | One conversation |
| `PUT` | `/api/conversations/{id}` | Update a conversation |
| `DELETE` | `/api/conversations/{id}` | Delete a conversation and its messages; refused while a reply to it is being written |
| `GET` | `/api/conversations/{id}/thread` | A chat as the page reads it: its messages, its branch points, and whether it ends in an unanswered question |
| `POST` | `/api/conversations/{id}/changes` | An edit, a regenerate or a branch of a chat (ADR 0017) |
| `POST` | `/api/conversations/{id}/pictures` | Keep a picture beside the one it was made from (ADR 0018) |
| `DELETE` | `/api/messages/{id}` | Delete a message and every message after it; refused while a reply is being written |
| `POST` | `/api/chat` | A chat's title: the text the model on a port answers a title request with (messages, a temperature and a token cap, and no other key) |
| `POST` | `/api/attachments` | Store an image, the raw body (a PNG or a JPEG of at most 8 MiB), and answer its id, type, size and estimated prompt tokens |
| `GET` | `/api/attachments/{id}` | A stored image's bytes, as they were sent |
| `POST` | `/api/images/generations` | Draw, as the proxy's `/v1/images/generations` does |
| `GET` | `/api/images/drawing` | Whether a message sent with Draw pressed can draw here, and this machine's image models; query `far`, `calls_tools`, `image_model` |
| `GET` | `/api/version` | Which build of gglib is answering |
| `POST` | `/api/benchmark/compare` | A compare run, streamed as SSE |
| `POST` | `/api/benchmark/perf` | A perf run, streamed as SSE |
| `POST` | `/api/benchmark/tune` | A tune run (a sampling-parameter sweep), streamed as SSE |
| `POST` | `/api/benchmark/tune/{run_id}/apply` | Judge a finished tune run against the apply gate and, when it passes, store the winner as the model's measured defaults |
| `POST` | `/api/benchmark/agentic` | A raw-vs-gglib A/B agentic eval, streamed as SSE |
| `GET` | `/api/benchmark/runs` | Recent benchmark runs |
| `POST` | `/api/remote/enable` | Bring the tunnel up and arm a pairing; answers the ticket and the code; 409 when already up |
| `POST` | `/api/remote/disable` | Take the tunnel down; idempotent |
| `GET` | `/api/remote/status` | The tunnel's status |
| `POST` | `/api/remote/invite` | Mint a key and a code for one new device |
| `GET` | `/api/remote/devices` | The paired devices |
| `DELETE` | `/api/remote/devices/{device}` | Stop admitting one device |
| `POST` | `/api/remote/join` | Bind a loopback port here that is the far machine's proxy, redeeming a pairing code when one is given |
| `POST` | `/api/remote/disconnect` | Close that port; idempotent |
| `POST` | `/api/remote/kill` | Stop the far daemon through the tunnel, then disconnect; the body must be `{"confirm":"shutdown"}` |
| `GET` | `/api/remote/chats` | The far machine's chats, through the tunnel |
| `GET` | `/api/remote/chats/{id}` | One of the far machine's chats |
| `PUT` | `/api/remote/chats/{id}/turns/{run_id}` | Add a turn to one of the far machine's chats |
| `GET` | `/api/remote/runs` | The far machine's runs |
| `GET` | `/api/remote/runs/{run_id}/events` | A far run's events (SSE), from `?after=N` |
| `POST` | `/api/remote/runs/{run_id}/cancel` | Cancel a far run |
| `GET` | `/api/remote/images/drawing` | Whether the far machine can draw for a turn on one of its chats, and its image models |
| `GET` | `/api/remote/models` | The paired machine's models, read through the tunnel, with what may be done to them there |
| `GET` | `/api/remote/models/{model}` | One of the paired machine's models, read in full |
| `POST` | `/api/remote/models/{model}/load` | Have one of the paired machine's models resident now |
| `POST` | `/api/remote/attachments` | Store an image on the paired machine, through the tunnel; nothing is kept here |
| `GET` | `/api/remote/attachments/{id}` | One of the paired machine's stored images, sent `no-store` |

## Usage

This crate is a library — it has no binary target. The daemon that mounts it
is `gglib daemon run`, on a fixed loopback port:

```bash
# Start the daemon (this crate's router is what answers)
gglib daemon run

# Or have any command that needs it start one for you
gglib up
```

## Design Decisions

1. **Axum Framework** — Chosen for async-first design and tower middleware ecosystem
2. **Shared backend** — Handlers call `gglib-app-services`, the facade `gglib-cli` and `src-tauri` use too
3. **Thin Handlers** — No logic, just parse → delegate → serialize
4. **CORS Support** — Configurable CORS for web UI development
