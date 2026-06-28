# Agent Chat

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-agent_chat-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-agent_chat-complexity.json)

<!-- module-docs:start -->

Interactive agentic chat handler for `gglib chat`.

Entry point: [`run`].  Sub-modules keep each concern small and
independently readable:
- [`config`]   — resolves LLM port + MCP tools, composes an [`gglib_core::ports::AgentLoopPort`]
- [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
- [`drain`]    — async event-stream consumer (spinner, thinking accumulator)
- [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input
- [`tool_format`] — tool-result summary formatters
- [`markdown`] — Markdown normalisation + termimad rendering
- [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-config-coverage.json) |
| [`drain.rs`](drain.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-drain-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-drain-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-drain-coverage.json) |
| [`markdown.rs`](markdown.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-markdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-markdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-markdown-coverage.json) |
| [`persistence.rs`](persistence.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-persistence-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-persistence-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-persistence-coverage.json) |
| [`renderer.rs`](renderer.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-renderer-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-renderer-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-renderer-coverage.json) |
| [`repl.rs`](repl.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-repl-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-repl-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-repl-coverage.json) |
| [`thinking_dispatch.rs`](thinking_dispatch.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-thinking_dispatch-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-thinking_dispatch-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-thinking_dispatch-coverage.json) |
| [`tool_format.rs`](tool_format.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-tool_format-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-tool_format-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-agent_chat-tool_format-coverage.json) |
<!-- module-table:end -->

</details>
