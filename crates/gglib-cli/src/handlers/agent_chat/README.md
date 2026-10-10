# Agent Chat

<!-- module-docs:start -->

Interactive agentic chat handler for `gglib chat`.

Entry point: [`run`].  Sub-modules keep each concern small and
independently readable:
- [`config`]   — readies the MCP tools, gathers the flags and the stored
  sampling layers beneath them, with the settings' agentic sampling switch
  for a model in the catalogue, and composes an
  [`gglib_core::ports::AgentLoopPort`]. It folds no ladder: the adapter's
  request pipeline does, once a request, as it does for the daemon's turns
- [`sampling_warning`] — says on stderr, with the session's first request,
  which sampling flags that fold passed over
- [`upstream`] — the llama-server a local session talks to: one already running
  here (`--port`) or one the daemon starts. Which *machine* answers is
  `crate::target`'s decision (ADR 0013), not this module's
- [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
- [`drain`]    — async event-stream consumer (spinner, thinking accumulator)
- [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input
- [`persistence`] — the saved conversation a session's turns are written to,
  through `gglib_app_services::transcript` as the daemon's agent runs are:
  the user's message when it is sent, and the reply when its turn ends,
  finished or not, rebuilt from the events the turn sent
- [`repl_line`] — what one line typed at the prompt asks for: a command, or
  the next message with the images attached to it
- [`branches`] — `/retry`, `/edit`, `/branch` and `/branches`: a change to the
  session's saved chat, made by the chat history service as the branching
  rules say (ADR 0017), in place or on a new branch the session goes on in;
  a turn that answers a question already saved saves only its reply
- [`images`]   — the images a turn carries: `--image` and the REPL's `/image`,
  each file through core's one ingest, with a receipt line on stderr; and,
  when a session starts, the daemon's check before a run
  (`AttachmentService::check_request`) over its history and first message
- [`sight`]    — whether the session's model can read an image, asked before
  the loop is composed: the catalogue row, or `/props` of a `--port` server
- [`tool_format`] — tool-result summary formatters
- [`markdown`] — Markdown normalisation + termimad rendering
- [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

Two of a turn's rules are core's, shared with the daemon, and only called
here. A session's iteration and stagnation limits are
`gglib_core::domain::agent::TurnLimits::resolve`: `--max-iterations`, then
the limit a resumed chat saved, then the stored settings, then the default. A
new chat saves only the limit its command line named. A chat's Thinking
choice is `gglib_core::domain::thinking::settle` (`resume_settings`), with
`--thinking on|off` as what the turn says: a choice named runs the session
and is remembered on the chat, new or resumed, through
`gglib_app_services::transcript::remember_thinking`, which the daemon's runs
write it with. With none named a chat switched off runs with a thinking
budget of `0`, and the resume says so on stderr when that sets aside a
budget its command line typed.

A third is asked of the daemon, since a session writes its chat's rows
itself. `--continue` on a chat the daemon is still replying to, for the page
or a paired device, is refused before anything is stored, in a sentence that
names the chat and the run: [`run`] reads a running daemon's listing of its
runs and applies `gglib_core::domain::runs::RunInfo::holds`, which the
daemon's registry refuses a second run by. No daemon is started to ask, and
one that does not answer with its runs, as another data root's does not, is
no refusal.

<!-- module-docs:end -->
