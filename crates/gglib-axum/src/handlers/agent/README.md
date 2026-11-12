# agent

<!-- module-docs:start -->

The server-side agentic loop: every agent run the daemon starts, this
machine's turn at `PUT /api/runs/{id}?kind=agent` (`gglib chat`, `gglib q`
and the page), a paired device's turn and a phone-kept chat run.

A door reads its turn into a `loop_request::LoopRequest`, which no body is
ever read as; `compose::prepare_over` applies the configured limits over the
upstream the run settled and calls
[`compose_agent_loop`](gglib_runtime::compose_agent_loop), so every door's
loop is composed alike. The limits are resolved by core's
`TurnLimits::resolve`, which the CLI reads too: the iteration limit the
chat's settings saved (named on the request by `hub_turn::plan`), then this
machine's stored `max_tool_iterations`, then the default; and the stored
`max_stagnation_steps`, which no turn names. The run's work (`run::work`)
logs each event as `compose::frame` writes it.

A request sent with `draw: true` (the page's Draw button) is the only one
whose model is offered `builtin:generate_image`: `compose::prepare_over`
composes the drawing tool for it, armed, and adds exactly that qualified name
to a tool filter that lists names. Such a run draws once: its first request
to the model offers that tool alone and demands a call (`tool_choice:
"required"`), and a first reply that does not call it, whether it calls
nothing or only some other tool, ends the run `failed`,
`image_generation_failed`, saying the model did not ask for the picture.
Once the tool has run, drawn or refused, the second request offers no tools
at all (no `tools`, no `tool_choice`, no grammar) and its reply ends the
run: its text is the answer, a tool call in it is not run, and a reply with
no text is answered "The model said nothing after the drawing tool ran."
(`gglib_mcp::DRAW_NO_WORDS`). That second request ends with one `user`
message the run's rows and events never hold, telling the model what to say:
`gglib_mcp::DRAW_THEN_SAY` when the tool drew (say in one or two sentences
what it asked the tool to draw, without apology or saying it cannot draw)
and `gglib_mcp::DRAW_THEN_SAY_FAILED` when the tool answered with an error
(say what went wrong). A later message of that chat, sent without `draw`,
is offered no image tool, and its request carries a note at the front that
the picture exists (`gglib_core::request_pipeline::DRAWN_NOTE`, added by
the endpoint's request pipeline and held by no row or event). When the model on the
other end reads images, that request and every later one also shows it the
most recent drawn picture, on the message that followed it
(`gglib_core::request_pipeline::shown_picture`, applied by the completion
adapter and held by no row or event); a model that does not is sent none and
is not refused for it. The tools a filter names are not offered to
such a run at any point. That holds for every run composed here with
`draw`: this machine's turn, a device's turn, and a chat run with builtins. Without
`draw` the tool is in no list and
cannot be called, `tool_filter: null` included. Every door first asks
`compose::refuse_unavailable_drawing`, before a slot is taken or a row
written: a model on another machine, or a machine that cannot draw, is
`400 drawing_unavailable` with the reason `GET /api/images/drawing` gives.

A request that draws may say which image model draws it, `image_model`, the
id of a row in that answer's `models`. `refuse_unavailable_drawing` is where
it is judged, for every door: without `draw` it is `400 invalid_request`,
and a model that is gone, is not an image model or lacks a file is `400`
with the answer's own code (`model_not_found`, `not_an_image_model`,
`image_model_incomplete`) and reason, never replaced by the default.
`prepare_over` places the id on the drawing tool (`DrawWith`), which sets it
as the model of the request it hands the image driver; the tool the chat
model is offered has no such argument, and nothing sent to the chat model
holds the id. Nothing is remembered here: the surface that keeps the choice
says it on each run that draws.

A run whose request the model's endpoint refused, this machine's or a
paired machine's, ends `failed` with the refusal's code when it is one of
gglib's own (`gglib_proxy::error_codes::is_gglib_code`), and `agent_error`
otherwise (`run::run_error`).

Inline `<think>` reclassification is handled upstream by
[`gglib_core::normalize::NormalizingStream`] in the LLM adapter, so this
handler only forwards already-typed [`AgentEvent`](gglib_core::domain::agent::AgentEvent)s.

An agent run stamps each turn's `turn_usage` event with the model it drove
(`gglib_app_services::transcript::MadeBy`: locally the catalogue's name and
quantisation for the model's id, on the paired machine the name and
quantisation that machine has for the model) before logging it, so the
frame a page draws and the row the reply is saved as say the same. A turn
on this machine's model also stamps the context its model was launched with
(`remote_upstream::endpoint`): read once the model is loaded, and only when
the primary slot's model is the turn's own by port and id. A run on the
paired machine's model, or on a model in the second slot, stamps none, and
the figure is left out: never a default. The loop itself counts the messages
it left out of each request and reports why the model stopped.

# Through the endpoint

Chat is a client of this machine's endpoint. A turn on one of its chats,
this machine's or a paired device's, and a phone-kept chat run with
builtins (`chat_turn`) load their model first, saying so with a `waiting`
event that names the model and, each second, how long the load has taken;
then drop the load's lease and dial the endpoint (`hub_turn::late_on`,
`remote_upstream::through_endpoint`): at the door's root
(`ProxyOps::door`, the endpoint turned on first when it is off, as starting
a model turns it on), with the key it demands, the model named by its
catalogue id. The turn is read from the catalogue's row for that id (its
name, quantisation, images and ladder), never from what the port serves by
the time it dials: the load's lease is gone, so the port may be empty or
serve another model, and the endpoint loads the chat's model again. A turn's
limit is written only when the endpoint's own ladder, with the sampling it
was started with and its default profile, names no `max_tokens`. Every
request of the loop is then a request to the endpoint
like any client's: shaped by its pipeline, guarded by its loop guard,
admitted on its queue, counted in its ledger (Endpoint › Recent, `gglib
proxy dashboard`) and in its one prompt-cache figure. Nothing holds the
model between requests: a swap between tool turns, a draw that evicts the
chat model or a recycle between turns makes the next request load it again,
and a reply waiting behind an image render waits in the endpoint's queue,
saying nothing. A key the endpoint does not admit (changed within its
settings cache's window) fails the run in this machine's own words
(`Door::key_refused`); an endpoint whose port another process holds fails
it `conflict` with the endpoint's reason; a pinned endpoint (`gglib serve`)
refuses a turn on another model with its `pinned_model_mismatch`. A
cancelled run drops the reply it is reading, so the endpoint ends that
request and lets go of the model.

The loop through the endpoint shapes nothing itself (`gglib_runtime`'s
proxied mode): it reads the images its messages name, writes Draw's
`required` on the first request, and limits each turn whose ladder names no
`max_tokens` to 16384 tokens (32768 for a `reasoning`-tagged model), or to
its thinking budget plus 8192 when that is more. A reply cut off there while
it writes a call, or before it writes any text, ends the run `failed`,
`turn_too_long`; one cut off in its text is taken as if it had stopped,
after a `system_warning`. The words after a Draw's picture are the exception
and are always answered for. A device's `max_tokens` is not read, so it can
neither raise nor lower the limit. The endpoint's loop guard alone counts
such a loop's trips; when it ends the run it answers in the model's place
with a finished message, which ends the run well with that sentence.

A run on the paired machine's model is not limited from this side: that
machine's own settings apply. It reports its own guard's decisions to this
machine's ledger.

# Which upstream

`remote_upstream` decides whether the loop drives this machine's endpoint
(`through_endpoint`, above) or a model of the machine on the other end of
the remote tunnel (`far`: that machine and the model's id there, the port
the tunnel bound, the key from the pairing as the bearer, and no shaping,
because the far proxy runs its own pipeline). A local run whose messages,
history included, carry an image is refused `400 model_cannot_read_images`
when its model has no projector (`image_gate`), and one on a model that
draws `400 image_model_cannot_chat`, before its loop is composed; a far run
is not judged here, since the far proxy refuses it by the same code. Not
connected, connected without a key, or connected to another machine than
the ref's is a `409` (`RemoteOps::far_for`), the last because that
machine's same id is another model.

On the far path the id is looked up there first (`FarProxy::lookup`), so a
model that machine does not have is its `404` before any turn starts; the
turns are sent with the id as the model, so no other model of its name
answers, and the run is counted under, and its turns made by, the name and
quantisation that machine has for it. Through this machine's endpoint the
model is likewise its catalogue id.

# Sampling

A turn through this machine's endpoint is resolved there, by its pipeline
(`gglib_core::request_pipeline::apply`, the one place the ladder is
folded), over what the turn names (`LoopRequest::sampling_layer`): its
thinking budget (thinking off is `reasoning_budget_tokens: 0`, which the
endpoint honours from any client) and, on a turn on a saved chat, the five
sampling settings the chat saved (`gglib chat`'s flags, `ChatSampling`),
sent as a client's, which the endpoint honours only while it trusts
clients' sampling. The chat's saved profile goes on the model the turn asks
for, `<id>:<profile>`, which the endpoint, or the paired machine's proxy,
routes to that profile whatever it trusts. A far turn is handed nothing this
machine stores; the far proxy resolves it by its own settings.

# Cancellation

An agent run, this machine's turn at `PUT /api/runs/{id}?kind=agent` or a
device's at the proxy's door, runs its prepared loop detached from any
response (`run::work`), so a client that goes away stops nothing: only
cancel or shutdown stops it, which drops the loop at its next `await`,
stopping token generation and any tool call in flight. It saves the
transcript to its conversation through `gglib_app_services::transcript`, the
one writer of a turn's rows.
Before it takes a slot, the images of the turn and of the history it
rebuilds are checked by their sizes alone (`AttachmentService::check_request`):
an id not stored is `400 attachment_not_found`, and images over 16 MiB
together `400 request_images_too_large`, on this machine's model or the
paired machine's, whose images are read here too.
A run holds no model (see "Through the endpoint"). A model server that goes
silent mid-reply for five minutes, the proxy's own idle bound, ends the run
with an error (one that never sends headers ends it at the ten-minute send
timeout). A turn's run
(`launch::launch_turn`, then `launch::begin_writes`) refuses a run on another machine than the one its
conversation ran on (a `409`: a chat's machine is fixed for its life, and a
paired machine is told apart by its fingerprint), saves the user's message,
names the run's model on the
conversation by its machine (a local run's registry id, a far run's id on the
paired machine, and its name in the settings) so the chat's next turn from
either door runs on it, or is refused for being the paired machine's, has
the conversation remember the Thinking choice its turn said and the folder
its run said, and starts the loop.

# The Thinking choice

Core's `gglib_core::domain::thinking::settle` is the one rule for it, read at
every door a turn comes through: `hub_turn::plan` reads a device's turn,
which is also how the page's turn on a paired machine's own chat arrives
there, and this machine's own, which `gglib chat --thinking` sends too. A turn says `off`,
`default` or nothing. `off` runs it with a
thinking budget of `0` and the conversation remembers; `default` runs it with
the model's own and the conversation forgets; nothing runs it as the
conversation remembers.
What is remembered is `ConversationSettings.thinking`, written by
`launch::begin_writes` beside the run's model, so a refused run, or a
repeated id that starts nothing, writes nothing. The write is
`gglib_app_services::transcript::remember_thinking`, which the CLI's chat
remembers a named choice through too.

# The folder

A chat's folder is where gglib's own file tools (`builtin:read_file`,
`builtin:list_directory`, `builtin:grep_search`) read in its runs; a run
with none is not offered them. Only this machine's own turn names one: its
body says `folder` on the turn that changes it, an absolute path to a
folder on this machine to set it, or `""` to forget it, and says nothing
otherwise. `run::settle_folder` settles it in `hub_turn::plan`, before a slot is taken: `""` forgets it
unchecked; a path goes through core's `folder::chosen` (absolute, exists,
a folder) and is the run's by its canonical path, refused `400
folder_unusable` when it fails; and with nothing said the run reads the
folder the conversation remembers, checked again and refused `409
folder_missing` when it is gone. Either refusal writes nothing. The
canonical path is what `launch::begin_writes` writes to
`ConversationSettings.folder` beside the Thinking choice
(`gglib_app_services::transcript::remember_folder`), so what the page shows
is what is read. The decision is
[ADR 0023](../../../../../docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md).

`compose::prepare_over` passes the folder to `compose_agent_loop` as the
file tools' root, whose descriptions name it, and `compose::with_folder`
adds the three qualified names to a tool filter that lists names; `[]`, a
model that calls no tool, stays `[]`, and `null` admits them already. Every
turn a paired device sends, and a phone-kept chat run, pass no folder,
whatever the conversation remembers: a device's turn calls
no tool of this machine's but the image tool its Draw asks for and the
tools of the MCP servers opened to paired devices, which name no file tool, and no
key of a device's turn names a path.

# A paired device's turn

`hub_turn` is the daemon's `AgentRunStarter`, handed to every proxy it
starts: `PUT /v1/runs/{id}?kind=agent` on the proxy's door carries only a
chat's id and the device's message, with any image named by the id its
upload answered, and the chat's Thinking choice on the turn that changes it;
a turn may be its images alone. An image the turn or the
history names that is not stored is `400 attachment_not_found`, images over
16 MiB together `400 request_images_too_large`, and an image for a model
with no projector `400 model_cannot_read_images`, all before the model is
loaded; a chat whose model draws images is `400 image_model_cannot_chat`
before anything else is read for it, and nothing is admitted. The history is rebuilt from the hub's
record as a resumed CLI chat reads it (core's `saved_history`: the system
prompt, then every row but a system one; then the message), the limits from the
conversation's settings (an iteration limit they do not name is this
machine's stored one), the tools `device_tools` offers it, never gglib's
own file tools, and the reply runs on the chat's model
(`hub_model`: the id its settings store, the one it was made with, the one
running on the hub (the one started last, of several), or the hub's default,
and never a name, so a renamed model still answers), loaded first when it is
not running, as an agent run in the device's scope saved to the chat.

`device_tools` says what a device's run is offered. Its floor is the image
tool for a turn that says `draw: true` (the device's Draw button), named
exactly `builtin:generate_image`, never the bare name, which would let an
MCP server's tool of that name through to a device; else nothing. The
chat's `no_tools` does not take the floor away, since the button is the
person's choice for that message, and a hub that cannot draw refuses the
turn `400 drawing_unavailable` before anything is written. A turn's
`image_model`, when it says one, is the page's: copied onto the loop's
request and judged there. Beyond the floor are the tools of the MCP
servers open to paired devices (`gglib mcp allow-remote <server>`, a flag
on each), read only while the tunnel's gateway says a session is live,
each by its qualified
`{id}:{tool}`: for a phone's chat, those of the servers its query names by
id, never every one, and none for `tools=none`; for a Mac's chat, every one
when it names no tool, only those its own list names exactly, by bare
name or as every tool of their server (`{id}:*`) otherwise, and none when it turned its tools off; and never a tool the
list names that no opened server has. An id a phone names that is not open
is dropped where the servers start, with one warning naming the ids, and
starts nothing. A turn is planned with the floor
alone; the rest is settled in the run's late step (`hub_turn::late_on`,
through `device_tools::settle`), once the model is found or loaded, where
an open server that is not running is started (`McpService::opened_tools`).
A `PUT` that is refused starts nothing. The decisions are
[ADR 0024](../../../../../docs/adr/0024-the-tunnel-opens-mcp-servers-by-name.md),
[ADR 0025](../../../../../docs/adr/0025-a-server-is-opened-to-paired-devices-by-a-flag-on-it.md)
and [ADR 0026](../../../../../docs/adr/0026-a-paired-devices-chat-run-names-the-open-servers-it-may-use-by-id.md).

A device's turn on a chat that stored the paired machine's model is
`409 conflict` once the chat has a saved message. Before that the model was
only chosen and the chat has run nowhere, as `transcript::keep_machine` reads it:
the turn runs on the one running here, or the hub's default, and never on
the name the chat stored, which is that machine's; with neither, it is
`422 no_model` like a chat that names none.

# This machine's own turn

`hub_turn::start` takes the turn's scope, and `RunScope::Local` is this
machine's own turn on one of its chats, the same `HubTurn` read the same
way, reserved before its model and saved alike. The page sends it at
`PUT /api/runs/{id}?kind=agent` (`hub_turn::start_local`) as a `LocalTurn`:
a `HubTurn`, key for key, and `folder`, and any other key refused `400
invalid_request` in fixed words that quote nothing of the body. So the page
names no history, port, model, tool filter or limit: the history is
`saved_history` over the chat's rows, the model is the one the chat saved
(`PUT /api/conversations/{id}` when the page's picker changes it, or the
conversation's `model` when it is made), and the tools and limits are its
settings'. The history the page used to send is pinned against
`saved_history` by `contracts/chats/history.json`. It differs from a
device's in three things only:

- **Model.** A chat that stored the paired machine's model runs there
  (`hub_model::paired`): the loop's request names it as `far`, the late step
  loads nothing here and `remote_upstream::far` takes the tunnel. The
  run is listed by the name the chat has for it, and nothing of it is
  looked up in this catalogue, so the image checks are that machine's
  proxy's to make. Draw on it is `400 drawing_unavailable`.
- **Tools.** `Asked::of_local` is planned without reading a server: every
  tool here (a `null` filter: every enabled server's and the builtins) when the chat names
  none, the tools it names when it names some, and none (`[]`) when it
  turned its tools off or the catalogue says its model calls none
  (`hub_model::calls_tools`; a model not in the catalogue, or the paired
  machine's, keeps its tools). The run's late step first starts the
  enabled servers it reads (`McpService::start_enabled`): every one, or
  only those whose ids the chat's list names when it names each tool as
  `{id}:{tool}`, and none when it calls no tool. The image tool and the
  file tools are added as the loop is composed.
- **Folder.** The turn may say the chat's folder, settled by
  `run::settle_folder`: `folder_unusable` and
  `folder_missing` before anything runs, the canonical path remembered once
  the run starts, and the file tools reading there. A device's turn has no
  folder: `HubTurn` names none, and `plan` refuses one said in its scope
  with `400 invalid_request`.

`chat_turn` is the starter's other door, the proxy's
`PUT /v1/runs/{id}?kind=chat&tools=<id>,<id>[&draw=true[&image_model=<id>]]`: a chat a paired
phone keeps itself, sent whole as an `OpenAI` request and run through the
loop so that it can use the servers it names and draw, its query read as a
`ChatQuery`. `tools=none` is such a chat with no server: offered no MCP
tool and starting no server, the image tool alone with `draw`, and no tool
at all without it. A `PUT` under the id of a run already started answers
with that run, so the run keeps the servers it started with. Its history is read by core's
`parse_openai_messages`, each inline image stored once as an attachment no
chat links (swept at the first daemon start a day or more later), its model resolved and loaded as a turn's
is, its tools those `device_tools` offers a phone's run, the image tool only with `draw` (drawn with the query's `image_model` when it names one), the request's
sampling not read except that `"reasoning_budget_tokens": 0`, a phone's
Thinking off, runs it with a thinking budget of 0, and no row written anywhere. Its run is a chat run in
the device's scope whose `frames` say `agent`.

The run exists before its model does (`launch::launch_turn`). The `PUT`
reads the turn, takes a slot, reserves the run and answers; finding or
loading the model, opening the endpoint's door, composing the loop and
writing the turn's rows (`launch::begin_writes`) are the run's own work. A cold load, or a load queued behind an
image render, is then a `waiting` event (`model_load`, naming the model and,
said again each second, how long the load has taken) in the run's log, not
a `PUT` that hangs past the bound a device or the tunnel gives it. What is
refused once the run exists ends it `failed`: `model_not_found`, naming
the model, when this machine does not hold it, and otherwise the code the
`PUT` answered before (`model_unavailable`, `unavailable`, `conflict`, an
endpoint that cannot be turned on among them). No row is written: the
turn's writes begin only once the loop is ready, and a run whose writes
never began saves no reply.

<!-- module-docs:end -->
