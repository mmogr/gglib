# Agent Chat

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-agent_chat-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-agent_chat-complexity.json)

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
- [`tool_format`] — tool-result summary formatters
- [`markdown`] — Markdown normalisation + termimad rendering
- [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

<!-- module-docs:end -->
