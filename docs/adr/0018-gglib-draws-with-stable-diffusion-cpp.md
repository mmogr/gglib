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
implementation of core's `ImageGenerationPort`. It resolves the model (the
one named, else the settings' `default_image_model_id`, else the only image
model with every file), refuses what the recipe refuses, admits the model,
takes the render turn, submits an `sd-server` job with `preview: proj` and
`preview_interval: 1`, and reads it once a second: Loading until the first
step, each step with its frame, Decoding, then the PNGs. `sd-server` cannot
interrupt a generating job, so a render whose reader left cancels the job
and, when it runs on, keeps the turn and the lease until it ends. A render
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
to the model: the first reply of a run sent with Draw must be the call. The
run's first request to the model offers `generate_image` alone and demands a
call (`tool_choice: "required"`, from `AgentConfig::first_call`); every
later request offers the run's whole tool list and leaves the choice to the
model. A first reply that still calls nothing ends the run `failed`,
`image_generation_failed`, saying the model did not ask for the picture,
and is never the run's answer. So does a first reply that calls only some
other tool; of one that calls `generate_image`, only its first call of it
is kept and every other call in that reply is dropped before anything runs,
so that reply starts one render. Under `auto` a small model wrote the call as
text in every run of one reading
([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-gglib-binary-draws-and-a-message-sent-with-draw-must-call-the-image-tool-first)).
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
the reason. A run that says `draw` is refused 400; the availability routes
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
- A render cannot be interrupted. Leaving it, or Ctrl-C in `gglib image`,
  discards the picture; the machine still finishes it. Stopping the image
  model ends it, at the cost of loading the model again.
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

## Out of scope

- A picture drawn from a chat, seen end to end. A gglib binary has drawn
  through `gglib image`, the streamed route and `/mcp`
  ([log-0018, 2026-10-11](log-0018.md#2026-10-11-a-gglib-binary-draws-and-a-message-sent-with-draw-must-call-the-image-tool-first));
  Draw from a chat drew nothing in that run, which is what decision 9's
  first-call rule answers, and it has not been run again since. Every test
  runs a scripted job or a stand-in server.
- MLX, ComfyUI, img2img, edits, LoRA, ControlNet, upscaling and video.
- A tool's image sent back to the model as context
  ([ADR 0016](0016-a-tools-image-is-an-attachment-on-its-tool-row.md)).
- Families beyond Flux.1, SDXL and Qwen-Image 2.1, and the variants of those
  sd.cpp loads differently. Flux dev reads as Flux.1 and takes schnell's
  steps; none was measured.
- Steps, guidance or a sampler per request or per model.
- CUDA, Vulkan and ROCm: their assets are matched by name only.
- Sizes above 1024²: the compute margins were read there.
- The benchmark arms, which take no generation turn.
- Whether one `completed` event per image is what an OpenAI client expects
  for more than one image: neither SDK's source says.
