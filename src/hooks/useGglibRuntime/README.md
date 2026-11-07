# useGglibRuntime

<!-- module-docs:start -->
React hook that drives the chat runtime from **runs**: replies the daemon owns
from start to end, so closing the page no longer stops one.

---

## Architecture

```
useGglibRuntime                      send / edit / regenerate / Retry / Stop
  │  PUT  /api/runs/{id}?kind=agent  (id minted here; the conversation exists first)
  │  POST /api/conversations/{id}/changes  (edit, regenerate: branchChanges)
  │  POST /api/runs/{id}/cancel      (Stop; leaving never cancels)
  └── useRunReader                   the open conversation's messages
        ├── open:  GET /api/runs → the agent run still going in it, if any
        │          GET /api/conversations/{id}/thread → saved rows, branch
        │          points, whether it ends in a question
        │          (sending waits for both)
        └── drawRun: GET /api/runs/{id}/events?after=0 → one frame per AgentEvent
              ├── text_delta / reasoning_delta → current assistant message
              ├── tool_call_start / _complete  → tool-call part, then its result
              │                                  and its images, on its artifact
              ├── tool_progress                → how far a long tool has got, on
              │                                  its part until its result
              ├── waiting                      → what the turn waits for, on its
              │                                  message until its prompt is read
              ├── prompt_progress              → how far the prompt has been read,
              │                                  on its message as the turn arrives
              ├── turn_usage                   → how the turn was made, on its
              │                                  message
              ├── system_warning               → a notice to the caller
              │                                  (onSystemWarning); the run goes on
              ├── event: preview (no seq)      → the tool's picture so far, held
              │                                  beside the messages, never in one
              ├── iteration_complete           → finalize, open the next message
              ├── final_answer / error         → settled
              └── event: run (the end)         → show the rows the daemon saved
```

A send is a turn (`LocalTurn`): the conversation and the new message, its
images by the ids their uploads answered, and nothing else. The daemon
rebuilds the history from what the conversation saved (`saved_history`,
pinned against what the page used to send by
`tests/ts/contracts/savedHistory.test.ts`), and runs the turn on the model,
tools and limits the conversation's settings name, loading the model first
when it is not running: a chat needs no served port. A conversation a send
makes keeps the model the chat opened on (`model`: this machine's, or the
paired machine's), and the composer's picker saves a new one as the
conversation's (`app/useChatSwitch`).

The daemon saves the user's message when a run starts and the reply when it
ends; the page saves no turn. A turn refused once its run exists (its paired
machine away, `conflict`; its model unable to load, `model_unavailable`)
writes nothing, so when the run a send started ends `failed` with no new
question among the saved rows, the text and images go back to the
composer, beside the run's reason. What `drawRun` draws is provisional: at the end
the thread becomes the saved rows, with their ids, tool rows folded, and an
unfinished reply marked, and how long each turn thought. The live run is
looked up before the rows load, so a run that ends during the opening is
shown once. Nothing about a run is kept in browser storage; its id lives in
memory while it is read.

An edit (of a question or of a reply) and a regenerate are changes the daemon
makes (`POST /api/conversations/{id}/changes`, ADR 0017), and the page holds
no rule of when one branches. A change that would rewrite a saved reply is
made on a new chat, a copy of the one it was made on as far as the change;
the answer names it, and the page lists and opens it and tells the person the
original is kept (`onBranched`). Only an edit of the last question, while
nothing answers it, is made in place. When the answer says the chat it left
is to be answered, a run answers it: `answer_saved` on the turn, with no
message, and the daemon runs from what is saved and saves only the reply.
A chat that ends in a question nothing answers (`answerable`) offers Retry,
which starts the same run. A refused change changes nothing, and the page
never deletes. Branch from here copies the chat as far as the end of a turn
into a new branch, answered by nothing. The thread says the branch points
the chat's family holds along it, and an option chosen at one opens its
chat as a branch is opened (`onConversationChanged`).

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
refused never calls it. The runtime keeps no choice itself and weighs nothing.

A run on this machine says the chat's folder when the caller's `folder` gives
one, asked as the run starts: `folder` on the run's body, a path or `""` for
Clear, and no key otherwise; a far turn never carries it. The runtime calls
that answer's `accepted` once the daemon has taken the run, with the chat it
saved to, and its `refused` when the daemon refused it, also with the chat a
first send made; a refusal for the folder (`folder_unusable`,
`folder_missing`) is told to the caller in the sentence `refused` gives.

A run that says `draw` also says `image_model` when what the caller's `draw`
gave names one (`imageModel`), and never without `draw`.

A send says `draw: true` when the caller's `draw` gives something at that
send, which is while the composer's Draw button is armed, on the run's body or
on a far turn's, and so does an answer run (an edit, a regenerate or a Retry)
started while it is armed; no key otherwise, and a run without it is offered no drawing
tool. The runtime calls the `accepted` it was
given once the turn is taken, as it does for the Thinking choice, and never
for a refused one.

A render's preview frames come on the run's stream as `event: preview`, beside
the log and with no seq. `drawRun` hands each to `useRunReader`, which holds
the newest for each tool call in `previews`, beside the messages: no message
ever holds one, so nothing that saves, sends or exports a message can. A
call's frame goes when its `tool_call_complete` is drawn, a frame that comes
after that is dropped, and none is held once the run ends or is no longer
read.

A chat with the paired machine's model (`model` names that machine) is another
thing: a conversation of this machine's, run here, whose turns the daemon
sends to that machine by the model's id there, which the conversation saved.
A conversation a send makes for it is made for that model, so its machine is
fixed from its first turn. While that machine is away its turn is a run that
fails coded `conflict`, shown as any failed run is.

All loop orchestration (context pruning, tool execution, stagnation detection,
loop detection) lives in the Rust `gglib-agent` crate.

---

## Module map

| File | Role |
|---|---|
| `useGglibRuntime.ts` | The runtime: send, edit, regenerate, Retry and Stop, as runs and changes; each start says the chat's Thinking choice when the caller gives one, and `draw` when the caller's Draw button is armed, and tells the caller once that turn is accepted; hands the page what the open chat says of its branches (`branching`) |
| `branchChanges.ts` | An edit, a regenerate or Branch from here as the change it asks the daemon for, and the change made: the chat it leaves shown, or the branch it made opened, and answered by a run when the daemon says so; with no change, Retry. `useBranching` hands the thread what the chat offers of its branches as one object, new only when the chat's thread is read again |
| `useRunReader.ts` | The open conversation's messages: finds its live run, loads the rows, attaches to the run, stops reading on leave, shows what was saved at a run's end; keeps how the run it last read to its end ended (`endedRun`) until the conversation is left, which is how the page knows a reply finished in front of it; keeps what the daemon says beside the rows (`answerable`, `points`); hands up each reading of a far chat, unless it was left first; holds the preview frames of the run it is reading (`previews`), and none between readings |
| `drawRun.ts` | Reads one run's events from the first and draws them; a `preview` item goes to the frames held beside the messages, until its call's result is drawn or the reading ends |
| `runPreviews.ts` | A run's preview frames, by tool call: a `preview` event's data read as a frame or not at all, the newest kept for each call, and a frame as an image's `src` |
| `runRequest.ts` | The turn's body (`LocalTurn`, `buildTurn`): the conversation, the message and its images by id, and no model, port, history, tool filter or limit; `thinking` only when the turn changes the chat's choice, and `folder` only when it changes the chat's folder (`""` for Clear); `answer_saved` only on a turn that answers the question the chat ends in; `draw` only for a turn started while Draw is armed: a send, or an answer turn (an edit, a regenerate or a Retry); `image_model` only beside `draw`, when the chat chose one. And the run id |
| `savedRows.ts` | A conversation's saved thread with its branch points and whether it ends in a question, its live run, and the row a message is; a far chat's from the far machine, handed on as that machine answered it, its live run from the far listing's `live_run` |
| `chatSource.ts` | Which machine a chat is on: that machine's runs (list, cancel, events) and image store (upload, read), and the text a turn carries, to a far chat or as an edit |
| `imageAttachments.ts` | The composer's image adapter: uploads an image when it is added (never when sent), says a refusal at once, turns a sent image into its stored id (only an id its own store answered: not one from the other machine's, as when the chat list moved there with images in the composer), and remembers each upload by its file so a draft handed back or carried over a model switch is not uploaded again |
| `imagePrep.ts` | An image read as the store reads it (a PNG's or a JPEG's size from its header), kept as it is within 2560 px and 8 MiB, else redrawn smaller by a downscaler passed in (the canvas by default) |
| `imageRefusals.ts` | The sentence for a refused image, at its upload or with its send, by the store's code; that a far gglib from before images cannot take one; and that an image was uploaded for another store than its chat's |
| `turnImages.ts` | A turn's images: read off a message, checked before a send (an image the chat's store does not hold, its upload failed or made for another store, sends nothing), and handed back to the composer with the text |
| `agentEventDispatch.ts` | One `AgentEvent` → message state; the switch `drawRun` runs per event |
| `agentMessageState.ts` | Pure state-mutation helpers for in-flight assistant messages; a tool result's images go on its tool-call part's `artifact`, by id and facts; a running tool's `tool_progress` on its part until its result, and a turn's `waiting` on its message until its prompt is read |
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

A turn names no limit, no tool filter and no reasoning control: the daemon
takes the conversation's saved settings (`PUT /api/conversations/{id}`),
then this machine's stored ones, then its defaults, as it does for a paired
device's turn, so a turn runs the same whichever client sent it. The
composer's Tools ▸ saves the chat's tools as they change
(`useChatTools`); a chat not yet made has its choice saved by the send that
makes it, before its turn starts (`tools`). The agent limits are Settings ›
Tools', and the reasoning effort and budget Settings › Models › Sampling
defaults'.

<!-- module-docs:end -->
