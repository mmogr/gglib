# useGglibRuntime

<!-- module-docs:start -->
React hook that drives the chat runtime from **runs**: replies the daemon owns
from start to end, so closing the page no longer stops one.

---

## Architecture

```
useGglibRuntime                      send / edit / regenerate / Stop
  │  PUT  /api/runs/{id}?kind=agent  (id minted here; the conversation exists first)
  │  POST /api/runs/{id}/cancel      (Stop; leaving never cancels)
  └── useRunReader                   the open conversation's messages
        ├── open:  GET /api/runs → the agent run still going in it, if any
        │          GET /api/conversations/{id}/messages → saved rows
        │          (sending waits for both)
        └── drawRun: GET /api/runs/{id}/events?after=0 → one frame per AgentEvent
              ├── text_delta / reasoning_delta → current assistant message
              ├── tool_call_start / _complete  → tool-call part, then its result
              ├── iteration_complete           → finalize, open the next message
              ├── final_answer / error         → settled
              └── event: run (the end)         → show the rows the daemon saved
```

The daemon saves the user's message when a run starts and the reply when it
ends; the page saves no turn. What `drawRun` draws is provisional: at the end
the thread becomes the saved rows, with their ids, tool rows folded, and an
unfinished reply marked, and how long each turn thought. An edit or a
regenerate names the edited message (or the regenerated question) as the
run's `replace_from`; the daemon replaces it and every later row with the
message only once it accepts the run, so a refused run changes nothing and
the page never deletes. The live run is looked up before the rows load, so
a run that ends during the opening is shown once. Nothing about a run is kept in browser storage; its
id lives in memory while it is read.

A far chat (`source: 'far'`) is the machine this one is joined to, read
through this daemon's `/api/remote/*`: its rows from `/api/remote/chats/{id}`,
its live run from the far listing, its events and Stop from
`/api/remote/runs/{id}/…`. A send is `PUT /api/remote/chats/{id}/turns/{run}`
with the new text and its images, by the ids the far machine's store
(`/api/remote/attachments`) answered; the far machine runs and saves the reply. A far chat
offers no edit, no regenerate and no new chat, and nothing of it is kept here.
Every reading of it (at opening, after a run, after a send it refused) is
handed to the caller (`onFarOpened`) unless the chat was left first: the far
list says nothing of a chat's settings, so that is where the page learns what
a far chat remembers and which model it ran on.

A send says the chat's Thinking choice when the caller's `thinking` gives one,
asked as the send starts: `thinking: "off"` or `"default"` on the run's body,
or on a far turn's, and no key otherwise. Once the daemon, or the far machine,
has accepted the turn that says it, the runtime calls the `accepted` the
caller gave with the choice, before the reply is read; a turn that was
refused never calls it. The runtime keeps no choice itself and weighs nothing:
the device-wide effort and budget go on every local run as they did, and the
daemon lets a chat's Off win.

A chat with the paired machine's model (`pairedModel`) is another thing: a
conversation of this machine's, run here, whose turns the daemon sends to that
machine by the model's id there (`far` on the run's body). A conversation a
send makes for it is made for that model, so its machine is fixed from its
first turn.

All loop orchestration (context pruning, tool execution, stagnation detection,
loop detection) lives in the Rust `gglib-agent` crate.

---

## Module map

| File | Role |
|---|---|
| `useGglibRuntime.ts` | The runtime: send, edit, regenerate and Stop, as runs; each start says the chat's Thinking choice when the caller gives one, and tells the caller once that turn is accepted |
| `useRunReader.ts` | The open conversation's messages: finds its live run, loads the rows, attaches to the run, stops reading on leave, shows what was saved at a run's end; hands up each reading of a far chat, unless it was left first |
| `drawRun.ts` | Reads one run's events from the first and draws them |
| `runRequest.ts` | The run's body (`AgentRunRequest`), and the run id; a turn on the paired machine's model carries it as `far`, that machine and the model's id there, and no name; `thinking` is in the body only when the run changes the chat's choice |
| `savedRows.ts` | A conversation's saved thread, its live run, and the row a message is; a far chat's from the far machine, handed on as that machine answered it, its live run from the far listing's `live_run` |
| `chatSource.ts` | Which machine a chat is on: that machine's runs (list, cancel, events) and image store (upload, read), and the text a far turn carries |
| `imageAttachments.ts` | The composer's image adapter: uploads an image when it is added (never when sent), says a refusal at once, turns a sent image into its stored id (only an id its own store answered: not one from the other machine's, as when the chat list moved there with images in the composer), and remembers each upload by its file so a draft handed back or carried over a model switch is not uploaded again |
| `imagePrep.ts` | An image read as the store reads it (a PNG's or a JPEG's size from its header), kept as it is within 2560 px and 8 MiB, else redrawn smaller by a downscaler passed in (the canvas by default) |
| `imageRefusals.ts` | The sentence for a refused image, at its upload or with its send, by the store's code; that a far gglib from before images cannot take one; and that an image was uploaded for another store than its chat's |
| `turnImages.ts` | A turn's images: read off a message, checked before a send (an image the chat's store does not hold, its upload failed or made for another store, sends nothing), and handed back to the composer with the text |
| `agentEventDispatch.ts` | One `AgentEvent` → message state; the switch `drawRun` runs per event |
| `agentMessageState.ts` | Pure state-mutation helpers for in-flight assistant messages |
| `wireMessages.ts` | `GglibMessage[]` → backend wire-format conversion; every user message names its images by id |
| `reasoningTiming.ts` | Tracks per-message reasoning segment durations |
| `clock.ts` | Monotonic clock abstraction for timing |
| `index.ts` | Public barrel export |

---

## Message-per-iteration model

One React `GglibMessage` (role `assistant`) is created for each backend
iteration.  Tool-calling iterations open a new message at `iteration_complete`;
the final-answer iteration closes the last message at `final_answer`.  This
preserves the multi-message UI layout from the previous client-side loop.

---

## Configuration

`useGglibRuntime` accepts optional overrides forwarded to the backend:

| Option | Backend field | Default |
|---|---|---|
| `supportsToolCalls` | `tool_filter: []` when `false` | all tools |
| Tools popover → Agent limits | `AgentConfig` fields, via `agentOverridesToWire()` | backend defaults |
| Tools popover → Reasoning | **top-level** `reasoning_effort` / `reasoning_budget_tokens`, via `reasoningOverridesToWire()` | resolved from the profile / model / global / floor layers |

The last row is the one that is easy to get wrong. Both reasoning controls sit
at the top level of `AgentRunRequest`, not inside `config` — they are per-turn
shape rather than agent-loop tuning, and `AgentRequestConfig` declares neither,
so a level routed through `config` would be dropped by serde without a word.
That is why the store has two wire mappers rather than one.

The page sends no iteration limit. The daemon takes the persisted
`maxToolIterations` setting for a run that names none, as it does for a
paired device's turn, so the limit is the same whichever client sent the
message.

Internal tuning parameters (`max_stagnation_steps`, `context_budget_chars`,
etc.) are controlled by the backend's `AgentConfig::default()` and are not
exposed to untrusted callers.

When `supportsToolCalls === false`, an empty `tool_filter` is sent so the
backend exposes no tools to the model.

<!-- module-docs:end -->
