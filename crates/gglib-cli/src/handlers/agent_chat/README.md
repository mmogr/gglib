# Agent Chat

<!-- module-docs:start -->

Interactive agentic chat handler for `gglib chat`.

Entry point: [`run`].  Sub-modules keep each concern small and
independently readable:
- [`config`]   — resolves MCP tools + sampling, composes an [`gglib_core::ports::AgentLoopPort`]
- [`upstream`] — the llama-server a local session talks to: one already running
  here (`--port`) or one the daemon starts. Which *machine* answers is
  `crate::target`'s decision (ADR 0013), not this module's
- [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
- [`drain`]    — async event-stream consumer (spinner, thinking accumulator)
- [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input
- [`repl_line`] — what one line typed at the prompt asks for: a command, or
  the next message with the images attached to it
- [`images`]   — the images a turn carries: `--image` and the REPL's `/image`,
  each file through core's one ingest, with a receipt line on stderr
- [`sight`]    — whether the session's model can read an image, asked before
  the loop is composed: the catalogue row, or `/props` of a `--port` server
- [`tool_format`] — tool-result summary formatters
- [`markdown`] — Markdown normalisation + termimad rendering
- [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

<!-- module-docs:end -->
