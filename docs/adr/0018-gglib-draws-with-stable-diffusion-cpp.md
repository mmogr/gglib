# ADR 0018 — gglib draws with stable-diffusion.cpp: one image runtime beside llama.cpp, the GPU given in turns, and a picture only for a message sent with Draw

- **Status:** Accepted
- **Date:** 2026-10-10
- **Depends on:** [ADR 0016](0016-a-tools-image-is-an-attachment-on-its-tool-row.md),
  [ADR 0015](0015-a-model-reads-images-through-its-projector.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0018](log-0018.md)

## Context

gglib ran one kind of model, a chat model on llama-server. A model that
draws needs another program, files beside its weights, minutes of the GPU
for one answer, and a way for a chat to ask for a picture and show it. Every
reading below is in [log-0018](log-0018.md), taken on one machine, an Apple
M4 Max with 128 GB; its harness and logs are on the owner's machine and not
in the repository, as the log says of each.

- **stable-diffusion.cpp draws correctly on Metal, and slowly.** At 1024²,
  Flux schnell q8_0 took 76.4 s in all, SDXL base 62.4 s, and Qwen-Image 2.1
  q8_0 547.6 s
  ([log-0018, 2026-10-09](log-0018.md#2026-10-09-stable-diffusioncpp-draws-correctly-on-a-mac-and-slowly)).
  Quantization did not change Flux's speed, q8_0 and q4_k within 0.5% of
  each other and fp16 19% slower, so q8_0 is the default.
- **A render and a chat share the GPU badly.** With Qwen3.8-27B answering
  while Flux sampled, the chat fell from 15.3 and 14.7 tokens a second to
  10.8, 9.9 and 9.5, and Flux's sampling rose from 65.8 s to 119.6 s (same
  entry, "Beside a chat model"). Both finished and nothing swapped: memory
  can be shared, the work cannot.
- **An image GGUF says nothing about itself.** The two measured hold zero
  key-value pairs, and `gglib model add` registered Flux schnell as a chat
  model
  ([log-0018, 2026-10-10](log-0018.md#2026-10-10-gglib-knows-an-image-model-by-its-tensors-and-fetches-its-family)).
- **`sd-server` is one process for one model, with a job API that reports
  steps.** `GET /v1/models` answers one fixed id, `sd-cpp-local`, and went on
  answering while a render ran: 61 probes of 61, the slowest 213 ms. It has
  no `/health`. A job reports no step before the first one finishes, about
  40 s into that Flux render, and its preview frames were 56 to 61 KB of
  base64
  ([log-0018, 2026-10-10](log-0018.md#2026-10-10-gglib-installs-sd-server-launches-an-image-model-beside-a-chat-model-and-gives-the-gpu-in-turns)).
  Its errors are not OpenAI's shape, and it drew an 8x8 image for a 7x7
  request rather than refuse it (the 2026-10-09 entry, "The server").
- **A tool's image already has a home.** [ADR 0016](0016-a-tools-image-is-an-attachment-on-its-tool-row.md)
  stores an image a tool returns as an attachment on its tool row, shows it
  to the user and never sends it to the model.
- **MLX was measured and not chosen.** mflux drew the same picture in 38.2 s
  and 38.5 s, against a second runtime with a Python environment, another
  weight layout and no OpenAI server of its own (the 2026-10-09 entry,
  "Against MLX").

## Decision

Thirteen decisions, in the order a picture is made.

### 1. An image model is known by its tensors, and its family is one fact

`ImageFamily::sniff` (`crates/gglib-core/src/domain/image_family.rs`) reads
a file's tensor table by stable-diffusion.cpp's own loader rules and answers
one of three families, `flux1`, `sdxl` or `qwen-image-2.1`, or none. A model
keeps it as `image_family`. Everything else about an image model follows
from that one fact: the files it needs, its recipe, the program that serves
it, what it may be asked to do. A model's runtime is derived from it
(`Model::runtime`), never stored, so there is no second field to disagree.

Amended on 2026-10-11: there are four families. Decision 18 adds `z-image`.

### 2. The files beside the weights are linked by role, through the one link rule

A family's recipe (`ImageFamily::recipe`) names the roles it draws with:
a VAE, text encoders, or none for an all-in-one checkpoint. A component is
linked to a model in a role by `checked_link`
(`crates/gglib-core/src/services/model_links.rs`), the function a projector
is linked through; the projector keeps the single header check
[ADR 0015](0015-a-model-reads-images-through-its-projector.md) decision 1
gives it, and only a component's file is read for its tensor table. A
download sniffs the family from the first 1 MiB of the weights, fetches the
recipe's companions in the model's own download group, and takes Q8_0 when
no quantization is asked.

### 3. A recipe holds the family's settings, and a request cannot change them

Steps, guidance, sampler and flash attention are the recipe's (4, 1.0 and
`euler` for Flux.1; 20, 7.0 and `euler_a` for SDXL; 20, 6.0, `euler` and
flash attention for Qwen-Image 2.1), given to `sd-server` at launch, where
they are the defaults of every request. A request chooses a prompt, a size,
a count of one to four, and a seed. The recipe's `SizeRule` judges the size
before anything queues (a multiple of 64 for Flux.1 and SDXL, of 32 for
Qwen-Image 2.1, from 256 to 1536 a side, 1024x1024 when unsaid), because
`sd-server` would round a wrong one instead of refusing it.

Amended on 2026-10-11: Z-Image's recipe is decision 18's: 8 steps, 1.0,
`euler`, flash attention in the diffusion model alone, and a multiple of 16.

### 4. `sd-server` is installed pinned, pre-built where there is a build

The pin is `master-948-228c707` (`PINNED_SD_RELEASE`,
`crates/gglib-runtime/src/sd/install/release.rs`), overridable by
`GGLIB_SD_RELEASE`. macOS, Linux x86_64 and Windows x64 take the release's
asset for the platform, exactly one or a refusal that names the matches;
anything else, or `--build`, builds the `sd-server` target from source.
Everything lives under `.sd/`, beside llama.cpp's `.llama/`. On macOS the
asset is the default because one run of each did not find it slower than a
source build (73.47 s of sampling against 82.06 s).

### 5. One launch path, and health asked per runtime

Residency launches an image model as it launches any model, through
`sd-server` instead of llama-server (`SpawnConfig`), after checking the
binary and every component before the queue. Readiness and the health
monitor ask the runtime's own path, `RuntimeKind::health_path`: `/health`
for llama-server, `/v1/models` for `sd-server`, whose answer counts only
when it names `sd-cpp-local`.

### 6. An image model sits in the second slot

It never takes an empty primary. It takes the second slot when the memory
check grants it, budgeted at its files plus its family's compute margin (7
GiB for Flux.1, 8 GiB for SDXL, 9 GiB for Qwen-Image 2.1), so a chat keeps
its model while a picture is drawn; otherwise it swaps into an evictable
primary, and where the primary is held and nothing is granted it is refused
`image_model_does_not_fit` at once.

Amended on 2026-10-11: this places an image model only while no other image
model is loaded or loading. While one is, decision 17 places it.

Amended on 2026-10-11: Z-Image is budgeted at 7 GiB, read that day
(decision 18).

### 7. The GPU is given in turns: one render, or the chats

The generation gate (`crates/gglib-runtime/src/process/admission/`,
`GenerationGate` in `crates/gglib-core/src/ports/generation_gate.rs`)
orders generation, not memory.

1. *What counts.* The LLM turns in flight are the explicit LLM turns plus
   every in-flight request on a llama-server resident. A lease on an
   `sd-server` resident never counts.
2. *Who takes a turn.* A proxy request counts through its lease. A local
   agent send takes an explicit LLM turn before it sends and holds it until
   its stream ends (`TurnHeld`); `gglib chat` holds one on a connection to
   the daemon (`GET /api/generation/turn`) when a daemon already runs; a
   send to a model on another machine takes none. A render takes a render
   turn with its image model's lease.
3. *Order.* A render starts when no render is held, no LLM turn is in
   flight and no gate waiter arrived before it. An LLM turn, and a
   llama-server serve or launch, go when no render is held and no render
   waiter arrived first, so new chats queue behind a waiting render and
   cannot starve it. A request waiting for a slot is not a turn.
4. *Progress.* A render step is queue progress, so nothing behind a long
   render gives up while it steps; a waiter expires under the same
   three-minute stall rule as an admission ticket.
5. *Waiting is said.* A waiting LLM turn is told the render's step, its
   total and its own place, and a run logs that as a `waiting` event.
6. *Teardown.* A render whose server has to be killed is retired in one
   order: the kill, then under one lock the slot emptied and the lease
   settled, only while the slot still holds that model, then the turn
   ended (`ProcessManager::retire_render`).
7. *A Stop asks.* A person's Stop on an image model that a render holds
   never empties the slot under the render. It asks; a render still
   waiting leaves the line with its lease, and one that is drawing is
   retired as rule 6 says at its next read of the job. A stop of the
   current model (a benchmark's, the proxy's cache clear) goes the same
   way when the primary slot holds an image model that is drawing. The
   Stop waits up to 20 seconds and answers an error, having emptied
   nothing, if the render has not let go. Not covered: the moment between
   a render's admission and its place in the line.

### 8. Everything that draws goes through one driver

`SdImageDriver` (`crates/gglib-runtime/src/sd/server/job.rs`) is the one
implementation of core's `ImageGenerationPort`. It resolves the model by
core's one rule ("One rule picks the image model" below: the one named,
else the settings' `default_image_model_id`, else the only image model with
every file), refuses what the recipe refuses, admits the model,
takes the render turn, submits an `sd-server` job with `preview: proj` and
`preview_interval: 1`, and reads it once a second: Loading until the first
step, each step with its frame, Decoding, then the PNGs. ~~`sd-server` cannot
interrupt a generating job, so a render whose reader left cancels the job
and, when it runs on, keeps the turn and the lease until it ends.~~
Retracted on 2026-10-11: such a render is stopped, as decision 15 says
([log-0018](log-0018.md#2026-10-11-a-render-nobody-waits-for-is-stopped-not-followed)). A render
with no new step for three minutes, or past thirty minutes, is retired.

Four doors stand in front of it and nothing else draws:

- `POST /v1/images/generations` on the proxy, OpenAI's Images shape, and the
  same handler at the daemon's `POST /api/images/generations`. With
  `stream: true` it sends `image_generation.partial_image` and
  `image_generation.completed` as OpenAI names them, and gglib's own
  `image_generation.progress` at every stage and step.
- `gglib image`, which draws through the daemon's route, so it works with
  the proxy stopped or keyed.
- The `generate_image` builtin (decision 9).
- The `/mcp` gateway's own `builtin__generate_image`, which asks the driver
  itself (decision 13).

### 9. `generate_image` is offered only to a message sent with Draw

The tool reaches a model's tool list only for a message sent with Draw
pressed: the web composer's Draw button, ggchat's, and `/draw` in `gglib
chat`. The wire is `draw: true`, on an agent run, on a device's turn
(`HubTurn.draw`), and as `&draw=true` on a chat run with builtins. Absent or
false, the tool is in no list and cannot be executed, whatever the request's
tool filter says, a filter of every tool included. The switch is for one
message: on the page it resets once a send is accepted, and a refused send
leaves it pressed; in `gglib chat` it resets after each send, however the
turn ends. A run that answers a question already
saved (an edit, a regenerate or a Retry), started while Draw is armed, draws
too, and a device's `answer_saved` turn may carry `draw` likewise.

The chat model writes the detailed prompt, calls the tool, and says what it
drew. The tool has no model argument; its result is one sentence for the
model and the images as attachments on its tool row
([ADR 0016](0016-a-tools-image-is-an-attachment-on-its-tool-row.md)), which
the model is never sent.

Pressing Draw is the person's explicit ask, so whether to draw is not left
to the model, and it is asked for once: a run sent with Draw is the call
for the picture, then the model's words. The run's first request to the
model offers `generate_image` alone and demands a call (`tool_choice:
"required"`, from `AgentConfig::first_call`). A first reply that does not
call it, whether it calls nothing or only some other tool, ends the run
`failed`, `image_generation_failed`, saying the model did not ask for the
picture, and is never the run's answer. Of a reply that does call it, only
its first call of it is kept and every other call in that reply is dropped
before anything runs, so that reply starts one render. Under `auto` a small
model wrote the call as text in every run of one reading
([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-gglib-binary-draws-and-a-message-sent-with-draw-must-call-the-image-tool-first)).

Once that call's tool has run, whether it drew or answered with an error,
the run's second request offers no tools at all: no `tools`, no
`tool_choice`, no grammar, and none of the other tools the run was composed
with. Its reply ends the run. Its text is the answer; a tool call in it is
not run; and a reply with no text is answered with fixed words, "The model
said nothing after the drawing tool ran." So a run sent with Draw asks the
model at most twice and starts at most one render. With the tools still offered
after the picture, one model asked for the same picture on every later
reply, five identical renders from one message, and another ended each run
with a tool call written out as text
([log-0018, 2026-10-11](log-0018.md#2026-10-11-draw-from-a-chat-draws-on-two-models-and-a-message-sent-with-draw-draws-once)).
Run against two real models, the rule held: 8 of 8 runs made one render and
ended in words. But the words denied the picture in 7 of the 8 ("I'm sorry,
I cannot draw images"). So the second request ends with one instruction
from the drawing side, a `user` message after the tool's result that goes
to the model for that request only and is in no row, no event and no later
request: to say in one or two sentences what it asked the tool to draw, or,
when the tool answered with an error, what went wrong
([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-run-sent-with-draw-draws-once-on-two-models-and-its-closing-reply-is-told-what-to-say)).
The instruction as built has not been run through gglib against a real
model.
An empty `size` argument is read as none, the image model's default.
A run ended for want of that call has still left rows on a door that saves
the chat: the
person's message, saved before the run, and what arrived of the reply,
marked incomplete (an empty row when nothing arrived). A chat run with
`tools=builtin` saves nothing.

A filter that names tools gains exactly `builtin:generate_image`, qualified,
never the bare name, which would also match an MCP server's tool of that
name and hand a paired device a tool the tunnel's owner did not open.

### 10. Draw overrides a chat's `no_tools` for that message

A device's turn sent with Draw is offered the image tool whatever the
chat's `no_tools` setting and the tunnel's `--allow-mcp` switch say. The
button is the person's explicit choice for one message, and drawing starts
no MCP server. MCP tools keep their switch.

### 11. One rule says whether a message can draw, and a run on another machine cannot

`drawing_availability` (`crates/gglib-core/src/services/drawing.rs`) answers
`GET /api/images/drawing`, the proxy's `GET /v1/images/drawing` and a run's
own `draw: true`, with reasons in one order: the chat's model is on another
machine; the model calls no tools; this process has no image driver; then
what the driver says (no image runtime, no image model, several and no
default, a file missing). Every refusal carries `drawing_unavailable` and
the reason, except that of an image model the question chose, which carries
the code a request naming that model is refused with ("One rule picks the
image model" below). A run
that says `draw` is refused 400; the availability routes
always answer 200, with the code and the reason in the body, and a client
greys its button with those words. Only the
availability routes are told whether the model calls tools: a run's own
`draw` is not refused for a model that calls none, and ends as decision 9
says when its first reply is no call.

A run whose model is on another machine does not draw: its loop would run
its tools here while the image model is there. A chat kept on that machine
draws with that machine's image model. A paired device reaches the image
routes through the tunnel.

### 12. A preview travels beside a run's log, never in it

A render is visible while it runs on every surface: that a picture is being
made, what waits behind it, a step count, and the picture sharpening.
Progress is logged: `tool_progress` (at most one a second, and at once on a
stage change, a new pass or a pass's last step) and `waiting`. A preview
frame is not. `RunLog::preview` keeps only the latest frame of a tool call
beside the log; a reader that has caught up is sent it as an SSE `preview`
event with no id, a reader that reconnects gets the current one once, and
it is gone in the step that logs that tool call's completion, or when the
run ends. No frame is
stored as an attachment, written to a transcript, or appended to a run log.
A tool that takes this long declares its own deadline
(`ToolDefinition.deadline`, thirty minutes for `generate_image`) instead of
the loop's.

### 13. `/mcp` reaches the tool only behind a switch that is off

The `/mcp` gateway lists `builtin__generate_image`, and will invoke it, only
while the `mcp_drawing` setting is on and drawing is available. It is off
unless set. The answer is the image inline and one sentence; nothing is
stored, since nothing would link it. A request that carried a
`progressToken` gets `notifications/progress` while it waits. No MCP server
can be named `builtin`.

A chat a phone keeps itself draws through
`PUT /v1/runs/{id}?kind=chat&tools=builtin&draw=true`: its unchanged OpenAI
request run through the agent loop here, with the image tool as its only
tool, nothing written to any chat, and its run marked `frames: "agent"`.
The request's sampling is not read; its Thinking choice,
`"reasoning_budget_tokens": 0`, is.

### 14. A chat that holds a drawn picture tells the model so on every later message

The message after a draw is sent without the image tool (decision 9), so
the model reads a history in which it called a tool it is not offered, and
says it cannot make images. Seven of nine such replies from one model did
([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-chat-that-holds-a-drawn-picture-tells-the-model-so-on-later-messages)).

So a request whose history holds a drawn picture carries one note for the
model: that the person pressed Draw and a drawing tool made a picture from a
prompt the model wrote, that the picture exists and the person can see it,
that a new or changed picture needs Draw pressed again, and that it is not
to deny the picture or write a tool call. The note is written into the
request on its way to the model. It is in no row, no event and no history a
run returns, and the person never sees it.

One stage of the request pipeline adds it (`request_pipeline::drawn_note`,
stage 1b), so every door has it from one place: the agent adapter, which
serves the page's run, a device's turn, a chat run with builtins and `gglib
chat`, and the proxy's plain chat path. It reads only the request's body.
The note is added when all of these hold:

1. `messages` holds an assistant call of `builtin:generate_image`, by that
   exact name, and a later `tool` message with that call's id whose text
   says a picture was drawn that the person can see. A tool message carries
   no success flag, so the text is the only evidence; a test in `gglib-mcp`
   holds the tool's sentences to the two marks the stage reads.
2. The request's last message is not one of the two closing instructions of
   decision 9. One that ends with either is the closing request of a run
   sent with Draw and is already told what to say. The stage knows it by
   those words, which live beside the note in `gglib-core`, because the
   roles alone do not tell the two apart: the message after a draw whose
   closing reply never arrived also ends with a tool's answer and a `user`
   message.
3. `tools` does not offer `builtin:generate_image`. A request that offers it
   is the first of a message sent with Draw and must call the tool.
4. No message already holds the note. A turn for a model on another machine
   is shaped here and again by that machine's proxy, and carries it once.

A chat whose draws all failed gets no note, since it would say a picture
exists. Another client's own `generate_image`, an MCP server's and the
`/mcp` gateway's `builtin__generate_image` are not a press of Draw and bring
none. A history from which the loop's pruning dropped the call or its answer
brings none.

The note goes at the front, as the system prompt: after a blank line at the
end of a first message that is a `system` one, or as a `system` message of
its own before everything else. It is added before capability coalescing,
so a model with no system role reads it as it reads any system message, and
before truncation, which measures its 488 characters with the rest. The
loop's own pruning runs earlier and does not count them. The stage can also
put the note at the start of the first `user` message after the picture;
one constant chooses, and the front was the better of the two when both
were sent to a model.

The front of the prompt therefore changes on the first message after a
chat's first picture, and that request reads the whole chat again. It
changes again for the two requests of each later message sent with Draw,
which carry no note, and back on the message after.

Two other ways were set aside. Folding the past call and its answer into
assistant text ("I asked the drawing tool for ...") renders in every
template, but shows the model drawing by writing a sentence, which it is
then apt to do when asked for a change (not measured); it also rewrites
turns the model produced.
Offering the tool on every message so the template describes it would have
the tool's definition say Draw was pressed when it was not, would switch on
the agentic sampling ceiling for the whole chat, would hand a device's turn
a tool its tunnel did not open, and on a dialect model `tool_choice:
"none"` comes back as tool markup in the text.

What this leaves. A chat a phone keeps itself sends no call and no tool message, so its
requests get none until ggchat sends them. ~~No model is sent the picture
itself (ADR 0016).~~
Retracted on 2026-10-11: a model that reads images is shown the most recent
picture, as decision 19 says
([log-0018](log-0018.md#2026-10-11-a-model-that-reads-images-is-shown-the-most-recent-drawn-picture)).

### 15. A render nobody waits for is stopped, not followed

Decision 8 kept a render whose reader had left until its job ended, with
the turn and the lease: a minute for Flux.1 and about nine for Qwen-Image
2.1, during which every chat waited. A person who pressed Stop on a phone
found the desktop still drawing
([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-render-nobody-waits-for-is-stopped-not-followed)).
That sentence of decision 8 is retracted, and this replaces it.

A render's reader is whoever awaits the driver's `generate`: a run, which
leaves when it is cancelled, as a Stop on the page or on a device cancels
it; `gglib image`, which leaves on Ctrl-C; a client of
`POST /v1/images/generations` or of the `/mcp` gateway, which leaves when
its connection does. A render has one reader. When it leaves before the job
has ended:

1. The driver asks `sd-server` to cancel the job, as before. A job still
   queued there, one already finished and one `sd-server` no longer knows
   are over: the turn and the lease go, nothing is killed, and the image
   model stays loaded.
2. A job that is generating cannot be cancelled (409). The render is
   retired at once, in one task and once, as decision 7's rule 6 says: the
   kill, then the slot emptied and the lease settled only while the slot
   still holds that model, then the turn ended. What waited for the GPU
   goes. A cancel that fails, or is not answered within five seconds, is
   read as a job that is generating.
3. The next request for a picture finds no image model loaded and loads it
   through the ordinary admission. That reload is the cost, and it was
   accepted.

A reader that leaves while its job is still being submitted comes to the
same end once `sd-server` has answered the submission. One that leaves
while it only waits, for the image model or for its turn, leaves the line
and stops nothing. One that leaves after the job completed changes nothing.

The person who pressed Stop sees what they saw before, a cancelled run and
no error. Decision 7's rule 7 stands: a Stop of the image model asks, and a
render that is being read retires itself at its next read of the job.

What this leaves. The kill is the one every server gets: a signal to end,
then up to five seconds (two on Linux) before the process is killed
outright, so the GPU is free when `sd-server` has exited, not at the moment
of the Stop. From reading the code and not from a run: a request for the
same image model that arrives while `sd-server` is being stopped, and
finds it still answering, is admitted to it, and its job then fails; so
does a second picture that already waited in line for that model. A
picture that was one step from done is lost like any other.

### 16. One rule picks the image model, and a person picks among them

A machine may hold several image models. Which one draws a request is one
rule, `resolve_image_model` in `crates/gglib-core/src/services/drawing.rs`,
and every door asks it. The first row that applies decides:

| | The model is | unknown | not an image model | lacks a file | otherwise |
|---|---|---|---|---|---|
| 1 | the one the request names, by id or name | refused `model_not_found` | refused `not_an_image_model` | refused `image_model_incomplete` | it draws |
| 2 | the settings' default image model | passed over | passed over | refused `drawing_unavailable`, naming what it lacks | it draws |
| 3 | the only image model with every file | | | refused `drawing_unavailable` when there is none, or several | it draws |

A named model is never replaced. When it is gone or lacks a file the request
is refused with why, before anything queues or loads, and neither the
default nor the only complete model is drawn with in its place: a person
chose it on purpose.

The availability answer of decision 11 lists the machine's image models,
`models`, each with its `id`, `name`, `family`, whether it is `complete` and
the roles it is `missing`, whether it is the `default` (the one a request
that names none would draw with, so at most one row, and none when such a
request would be refused) and whether it is `resident` (loaded now). The
key is always in the answer, so its absence tells a client the machine is an
older gglib; the rows are there whenever the driver could read them, also
when `available` is false, so a greyed button can still offer the others.
Asked with
`?image_model=<id>`, the answer judges that choice by row 1 and refuses it
with row 1's code, still as a 200; the far passthrough sends the query on.
A paired device already reads every image model's name and catalogue id from
`/v1/models`; the list adds its family, whether it has its files and which
roles it lacks, which one is the default and which is loaded. It carries no
path and nothing about a model that chats.

The choice belongs to the message, not to the machine. It is to travel as
`image_model` beside `draw` on each message sent with Draw, and the surface
that sent it remembers it for that chat: the page per conversation, a phone
on its own conversation, `gglib chat` for its session. gglib stores no
image model per chat and syncs none between devices; the machine's default
is a setting, and it is the only one. The wire is
`image_model`, the row's `id`, optional and meaningful only with `draw`;
this decision's first change makes the rule and the list, and the field on
a run's request follows it.

The field is on every door a message sent with Draw comes through: the
page's run and a device's hub turn carry it in the body, a phone-kept chat
run in its query, and a far chat's turn is handed on with it untouched for
the far machine to judge. It is judged by row 1 of the rule and refused with
that row's code before anything is written or loaded. `image_model` without
`draw` is refused `invalid_request`: a run without Draw is offered no image
tool, and the choice would be dropped in silence. A hub turn still refuses
a key it does not know, so a client sends `image_model` only to a machine
whose answer listed `models`. On the page, picking a model presses Draw for
the next message. `gglib image -m` and `POST /v1/images/generations` named
their model before this and are unchanged.

The model that calls the tool never picks. `generate_image` has no argument
for the image model, in a run and through `/mcp` alike: whoever calls the
tool is a model, and the image model is a person's choice or the machine's
default. An `/mcp` client draws with the default rule.

Two image models are not meant to be loaded together: picking the one that
is not loaded swaps it in. ~~This decision changes nothing in how a model is
placed (decision 6), and decision 6 alone does not always swap the right
one out; that is recorded in the log and left to its own change.~~
Retracted on 2026-10-11: decision 17 is that change
([log-0018](log-0018.md#2026-10-11-one-image-model-is-loaded-at-a-time-and-a-picked-one-replaces-the-idle-one)).

### 17. One image model is loaded at a time

Decision 6 placed an image model without asking whether another image model
was loaded, and its memory check read free memory with that one still
counted as used. From reading the code, never from a run
([log-0018, 2026-10-11](log-0018.md#2026-10-11-one-image-model-is-loaded-at-a-time-and-a-picked-one-replaces-the-idle-one)):
with a chat model and Flux.1 both idle, a Qwen-Image 2.1 with no room beside
both evicted the chat model and kept Flux.1; under a chat model a run held
it was refused `image_model_does_not_fit`; and two image models could end
up loaded.

While another image model is in a slot, loaded or still loading, that slot
is the only one an image model may take, whatever the memory check says of
it (`QueueState::image_slot` in
`crates/gglib-runtime/src/process/admission/state_placement.rs`):

1. *Idle.* The newcomer swaps in there, under the ordinary turn rules. The
   chat model is not evicted, and a chat model a run holds does not refuse
   the request.
2. *Rendering.* A render holds its model's lease from its admission until
   it ends, while it waits for its turn and while it draws. The newcomer
   waits for that lease; the model is not evicted under its render. A
   render step is queue progress (decision 7), so the wait does not expire
   while the render steps.
3. *Loading.* The newcomer waits until the launch lands or fails. A slot
   that is loading names its runtime for this.

With no other image model in a slot, decision 6 places it as before.

What this leaves. The memory check does not decide the swap. An image model
that needs the chat model's memory as well as the other image model's is
still loaded in the other's place, beside the chat model, as decision 6's
swap into the primary is made with no check that the model fits there; a
person who wants that picture stops the chat model first. A newcomer whose
launch fails has already stopped the model it replaced. After a render is
retired (decision 15) its slot is empty, so decision 6 places the next image
model, against a free reading that may still count the server being
stopped. None of it has been run with a real model.

### 18. Z-Image is a fourth family

Decision 1 knew three families and "Out of scope" left the rest out. Z-Image
is added, for Z-Image Turbo, from a header and stable-diffusion.cpp's guide
and source. **No Z-Image picture has been drawn**, through gglib or by
`sd-server`
([log-0018, 2026-10-11](log-0018.md#2026-10-11-z-image-is-a-fourth-family-recognised-from-a-header-and-never-drawn)).

1. *How it is known.* `z-image` is a file with `cap_embedder.0.weight` that
   has neither `local_decoder.out_conv.weight` nor
   `text_encoders.llm.connector.layers.0.self_attn.q_proj.weight`: with the
   first sd.cpp loads Z-Image L2P, which has no VAE, and with the second
   Ming Image. gglib also asks that `x_embedder.weight` takes 64 input
   features and `cap_embedder.1.weight` 2560, the two widths the recipe's
   VAE and text encoder feed; sd.cpp does not ask this, it sizes the model
   from them. A file that fails either is no family.
2. *Its companions.* The Flux.1 VAE, the same file from the same repository
   as Flux.1's, so a library that holds one fetches nothing; and Qwen3 4B as
   `llm`, by default `unsloth/Qwen3-4B-Instruct-2507-GGUF`
   `Qwen3-4B-Instruct-2507-Q8_0.gguf`. The `llm` role takes a GGUF of
   architecture `qwen3` whose `token_embd.weight` is 2560 wide and nothing
   else: Qwen-Image 2.1's Qwen3-VL 8B is refused, and so is a safetensors
   encoder, which sd.cpp can load and gglib has not read.
3. *Turbo's settings, for the whole family.* 8 steps, guidance 1.0, `euler`,
   flash attention in the diffusion model alone (`--diffusion-fa`, where
   Qwen-Image 2.1 is given `--fa`), sizes a multiple of 16. A base Z-Image file is
   expected to be the same family, since sd.cpp loads both as one model; no
   base header was read. It is then drawn with Turbo's settings, where
   sd.cpp's guide runs it at guidance 5.0. Settings per model are still out
   of scope.
4. *Its compute margin is 7 GiB*, read on 2026-10-11: the largest buffer of
   a 1024x1024 Turbo render was the decode of this same VAE, 6,657 MB, as
   Flux.1's was. The log has the reading.

What this leaves. Whether a Z-Image file written with
diffusers' tensor names, which sd.cpp renames as it loads, is known: its
patch embedder has another name, and gglib would answer no family.

### 19. A model that reads images is shown the most recent drawn picture

A picture is saved on its tool row, and a tool message is sent to the model
as text, so a model with a projector could not see the picture it had just
had drawn: "what is in the corner?" was answered from the prompt it wrote.
ADR 0016's decision 5 said a tool's image is never sent to a model. The
owner reversed that for this one case on 2026-10-11
([log-0018](log-0018.md#2026-10-11-a-model-that-reads-images-is-shown-the-most-recent-drawn-picture)).

The rule is one function, `request_pipeline::shown_picture`, over the
messages a request is built from:

1. Only for a chat model that reads images (a projector linked). Any other
   model is sent what it was sent before, the note of decision 14 included,
   and is never refused for a picture a tool made: the refusal for a model
   with no projector counts the images a person attached, and no others.
2. Only a picture `builtin:generate_image` made: the images of a `tool`
   message that answers a call of that exact name. Another tool's image, an
   MCP server's `generate_image` among them, is shown to no model.
3. Only the most recent one, the last image of the last such message. An
   older picture stays as its tool message's sentence.
4. It goes on the first `user` message after that tool message, which is the
   message the person sent next. That is the same message on every later
   request. When another picture is drawn, the newer one goes on the message
   after it and the older message is sent as it was written.
5. Nothing is added while no `user` message follows the picture, and nothing
   on the closing request of a run sent with Draw, whose last message is
   decision 9's instruction and not the person's.

The message that carries it is sent as parts: one line of text, the picture
as an `image_url` part, then the person's own text and images as they would
have been sent. The line is `request_pipeline::DRAWN_PICTURE_LINE`: "(Added
automatically, not written by the person: the image below is the most recent
picture the drawing tool made in this conversation, the same one the person
sees. You can see it now, although the tool's answer said you could not. The
person's own message follows.)" It says whose the image is, so the model
does not take it for something the person attached, and it takes back the
tool's "the user can see it, you cannot" for this picture. The tool's answer
is sent as it was saved and no row is rewritten; the note of decision 14 is
unchanged, and is true with the picture shown or not.

A `user` message, because an OpenAI `tool` message is text and whether
llama-server reads an image part in one is unverified (ADR 0016's second
reading, not taken). A message of its own straight after the tool's answer
was set aside: a `user` message between a tool's answer and the reply to it
is a shape no template here has been sent.

**Where.** The completion adapter (`gglib-runtime`,
`ports_impl/llm_completion/images.rs`) asks the rule for each request it
builds, reads the picture from the attachment store and writes the two
parts into that request's body. It is the one place an image's id becomes
its bytes, and every agent loop sends through it: the page's run, a device's
turn on a hub chat, a chat run with builtins and `gglib chat`. The messages
themselves are not changed, so the picture is in no row, no event and no
history a run returns. For the ids to reach the adapter, `AgentMessage::Tool`
carries `images`: a saved tool row resumes with them, the loop keeps a
result's, and the page sends them on its tool entries. Whether the model
reads images is `ModelContext::image_input`, from the catalogue row; a model
the catalogue does not hold is shown nothing.

**Seeing is not drawing.** A request that shows the picture offers the
image tool exactly when decision 9 says so, which is only a message sent
with Draw. Asked for a change without Draw, the model can now say what to
change from the picture itself, and decision 14's note still has it tell
the person to press Draw.

**What it costs.** The picture is priced as any image a model reads
(ADR 0015): 1,024 prompt tokens for 1024 by 1024, at most 4,096. The request
pipeline's budget counts it, since it measures the body; the agent loop's
own pruning estimate and the doors' check of a request's images before a run
do not, since they count the messages, which do not hold it. The adapter
counts it against the request's 16 MiB after the person's own images and
leaves it out, with its line, when it would not fit. A picture that is no
longer stored or cannot be read is left out the same way. Neither ends the
request. The message that carries the picture is new on the request that
first carries it, so showing it changes nothing earlier in the prompt. Once
another picture is drawn the older message is sent without its picture,
from that run's closing request on, and that one request reads the prompt
again from the older message where it would otherwise have read only what
the run added. The message after it reads the whole prompt again in any
case, as decision 14's note returns to the front.

What this leaves.

- A run on the paired machine's model is shown the picture when that
  machine's answer to the model lookup says `imageInput`; its proxy receives
  an ordinary image part. `gglib chat --remote` knows nothing of its model
  and shows none.
- A chat a phone keeps itself has no tool row here. Its client holds the
  picture and shows it by sending it as an image part of its own message
  ([docs/clients.md](../clients.md#images)); gglib then adds nothing.
- The plain proxy path adds no picture to any request.
- **No real model has been shown a drawn picture through gglib.** Every
  test reads the request a scripted server received. How a chat template
  renders two text parts around an image in a `user` message that follows a
  tool exchange, and whether a model takes the line as the person's words,
  are unread.

## Consequences

- A person presses Draw, sends a message, and sees a picture arrive in the
  reply, with a bar and a sharpening frame on the way. A model never starts
  a render on its own, and never declines one that was asked for: a model
  that cannot be made to call the tool gives a failed run and a sentence
  saying so.
- While a picture is drawn nothing else generates. A chat sent meanwhile
  is answered after. An agent run and `gglib chat` say they are waiting,
  with the render's step; a plain request through the proxy waits without
  a sign. A Qwen-Image 2.1 picture holds the chats for about nine minutes
  at 1024².
- ~~A render cannot be interrupted. Leaving it, or Ctrl-C in `gglib image`,
  discards the picture; the machine still finishes it. Stopping the image
  model ends it, at the cost of loading the model again.~~
  Retracted on 2026-10-11: leaving a render stops it, at that same cost
  (decision 15, [log-0018](log-0018.md#2026-10-11-a-render-nobody-waits-for-is-stopped-not-followed)).
- A device's turn exists before its model loads, so its `PUT` answers at
  once, and what used to be refused by the `PUT` (`model_unavailable`,
  `image_model_cannot_chat`, `unavailable`) may instead end the run.
- A chat on a model served by another machine has a greyed Draw button and
  a sentence saying why.
- gglib installs and updates a second runtime. Its pin moves by hand.
- An image model's settings are its family's. A person who wants other
  steps or another sampler cannot ask for them.
- Drawing through `/mcp` is off until someone turns it on, so an MCP client
  cannot spend minutes of the GPU unasked.

## Kill criteria

- **Previews cost too much.** `sd-server` reports steps only with a preview
  mode on, so the driver asks for `proj` at every step. Read the server's
  own `sampling completed` lines for the same render with `preview` `none`
  and `proj`, on a quiet machine. If `proj` adds more than 5% to sampling,
  set `preview_interval` to 2. A first reading, under load, was inconclusive
  ([log-0018, 2026-10-10](log-0018.md#2026-10-10-gglib-draws-one-driver-behind-a-route-a-command-and-a-tool-and-only-for-a-message-sent-with-draw)).
  The quiet one
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-on-a-quiet-machine-a-preview-at-every-step-costs-nothing-that-four-renders-can-measure))
  put the means at 68.96 s without previews and 69.54 s with `proj`, 0.8%
  apart, with the two `proj` renders 11.24 s apart: no cost that can be
  measured at four steps, so the interval stays 1. It is two renders a mode,
  of Flux schnell only, and its first render may include a load of weights
  the others do not. What reopens it: a quiet reading, on Flux or on a
  twenty-step family, in which the renders of one mode differ by less than
  the gap between the modes' means and `proj` adds more than 5%.
- **The turns are not worth their wait.** The gate rests on the 2026-10-09
  reading of a render and a chat together: the chat at 9.5 to 10.8 tokens a
  second against 14.7 to 15.3 alone, and Flux sampling in 119.6 s against
  65.8 s. Take the same pair again on a later sd.cpp. If the chat beside a
  render stays inside its range alone, 14.7 to 15.3 tokens a second on that
  machine, the turns buy nothing, and chats may generate beside a render.
- **The asset is slower than a build.** One run each put the macOS asset at
  73.47 s of sampling and the source build at 82.06 s, under load. A repeat
  of both on a quiet machine in which the asset samples slower than the
  source build makes the source build the macOS default.
- **The shown picture misleads the model.** Decision 19 has been sent to no
  real model
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-model-that-reads-images-is-shown-the-most-recent-drawn-picture)).
  Send the saved shape of a draw turn and then "what is in the corner?" to a
  vision model's llama-server, three seeds, with the line and the picture
  on that message. If a reply treats the line as the person's words, or says
  it cannot see the picture, change the line's wording first. If
  llama-server refuses the message or the model describes nothing of the
  picture, take ADR 0016's reading of an image part in the `tool` message;
  if that is read, the picture moves there and the line goes.

## Out of scope

- Decision 9's second request, seen with a real model. A gglib binary has
  drawn through `gglib image`, the streamed route and `/mcp`
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-gglib-binary-draws-and-a-message-sent-with-draw-must-call-the-image-tool-first)),
  and from a chat: the demanded first call was made in 8 of 8 runs on two
  models, through the page's door, a chat run with builtins and `gglib
  chat`
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-draw-from-a-chat-draws-on-two-models-and-a-message-sent-with-draw-draws-once)).
  What follows the picture went wrong in that run. The rule that answers
  it, a second request with no tools, then made one render in 8 of 8 runs
  on the same two models, with closing words that denied the picture in 7;
  the instruction that answers that has been sent to llama-server directly
  and to a scripted model through gglib, never through gglib to a real one
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-run-sent-with-draw-draws-once-on-two-models-and-its-closing-reply-is-told-what-to-say)).
  A device's hub turn, a real tunnel and a reasoning model
  have not drawn at all. Every test runs a scripted job or a stand-in
  server.
- MLX, ComfyUI, img2img, edits, LoRA, ControlNet, upscaling and video.
- ~~A tool's image sent back to the model as context
  ([ADR 0016](0016-a-tools-image-is-an-attachment-on-its-tool-row.md)).~~
  Retracted on 2026-10-11 for the drawing tool's most recent picture
  (decision 19). Any other tool's image, and an older drawn picture, are
  still sent to no model.
- ~~Families beyond Flux.1, SDXL and Qwen-Image 2.1, and the variants of those
  sd.cpp loads differently.~~ Flux dev reads as Flux.1 and takes schnell's
  steps; none was measured.
  Amended on 2026-10-11: decision 18 adds Z-Image
  ([log-0018](log-0018.md#2026-10-11-z-image-is-a-fourth-family-recognised-from-a-header-and-never-drawn)).
  Families beyond those four are out, with the variants of the four sd.cpp
  loads differently, Z-Image L2P among them.
- Steps, guidance or a sampler per request or per model.
- CUDA, Vulkan and ROCm: their assets are matched by name only.
- Sizes above 1024²: the compute margins were read there.
- The benchmark arms, which take no generation turn.
- Whether one `completed` event per image is what an OpenAI client expects
  for more than one image: neither SDK's source says.
