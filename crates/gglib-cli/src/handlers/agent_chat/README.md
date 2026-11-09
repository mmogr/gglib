# Agent Chat

<!-- module-docs:start -->

Interactive agentic chat handler for `gglib chat`, a client of the daemon's
door.

The daemon runs the only agent loop. A session makes or resumes a chat with
the settings its command line names, and sends each message as the chat
page's turn (`PUT /api/runs/{id}?kind=agent`); the daemon answers it on the
chat's model, tools and limits, through the endpoint, and saves it. The
run's frames come back as the events they were logged from and are shown
as they always were.

Entry point: [`run`].  Sub-modules keep each concern small and
independently readable:
- [`turn`]     — one turn: put to the daemon, its run's frames read back as
  [`gglib_core::AgentEvent`]s into the drain, Ctrl+C a cancel of the run
  that then waits for it to end, and a refusal, a failure, or a stream that
  stopped before the run did, each said in one line
- [`resume_settings`] — what a session saves on its chat: a new chat what
  its command line names, a resume what the chat saved with what its
  command line names in its place; and `resume_machine`, which machine a
  resume goes back to
- [`persistence`] — the chat's row: made, its settings and system prompt
  replaced; the daemon saves the turns
- [`sampling_warning`] — the one line, before the first turn, that says
  the endpoint uses its own sampling until "Let clients set sampling" is on,
  and the one that names a sampling flag no chat keeps
- [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output,
  and a reply's figure line ("41 s · 13 t/s · 2,100 tokens reused", core's
  `TurnUsage::figure_line`, the GUI thread's words)
- `progress_line` — a long tool's progress ("drawing: sampling 2/4") and a
  wait before a reply ("Loading Qwen3-4B… 12 s", core's words), as one
  stderr line each report rewrites; a preview frame is not shown
- [`drain`]    — async event-stream consumer (spinner, thinking accumulator);
  prints the reply's figure line on stderr after it, from its last turn's
  usage, unless `--quiet`
- [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input,
  each line a turn on the session's chat
- [`repl_line`] — what one line typed at the prompt asks for: a command, or
  the next message with the images attached to it
- [`branches`] — `/retry`, `/edit`, `/branch` and `/branches`: a change to the
  session's saved chat, made by the daemon (`POST
  /api/conversations/{id}/changes`) as the branching rules say (ADR 0017),
  in place or on a new branch the session goes on in, and on a new branch
  while a reply is still being written; the turn after a change that leaves
  a question says `answer_saved`, and adds no message
- [`images`]   — the images a turn carries: `--image` and the REPL's `/image`,
  each file through core's one ingest, with a receipt line on stderr;
  whether the chat's model can read them is the daemon's to say
- [`draw`]     — `/draw`, the session's Draw switch: it asks the daemon
  whether it can draw and arms the next message, whose turn says `draw`,
  and is off again once that message is sent; `/draw <model>`,
  `/draw default` and `/draw list` choose, forget and list the image model
  the session draws with, held by the switch for the session and said on
  each turn that draws as its `image_model`
- `files`      — the folder gglib's own file tools read: for a new chat the
  directory the session was launched in, never home or `/`
  (`gglib_core::domain::folder::launched`), said on each turn, and only for
  a `--tools` list that admits a file tool; for a resumed chat the folder it
  remembers while that is still there, its turns saying none, and else the
  launch directory; and the one `Files:` line on stderr that
  names it, says it is read on this machine for `--remote`, or says why
  there is none
- [`tool_format`] — tool-result summary formatters
- [`markdown`] — Markdown normalisation + termimad rendering
- [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

A chat's limits and Thinking choice are the daemon's to resolve, by the
rules it resolves the chat page's turns by: `--max-iterations` is saved
with the chat, and `--thinking on|off` is said on each of the session's
turns, which the daemon remembers on the chat.

One thing is asked of the daemon before anything is stored. `--continue`
on a chat the daemon is still replying to, for the page or a paired
device, is refused in a sentence that names the chat and the run: [`run`]
reads a running daemon's listing of its runs and applies
`gglib_core::domain::runs::RunInfo::holds`, which the daemon's registry
refuses a second run by. No daemon is started to ask, and one that does not
answer with its runs, as another data root's does not, is no refusal. The
daemon is found, or started, once the model is known and before the chat
is written.

<!-- module-docs:end -->
