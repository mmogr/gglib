# agent

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-complexity.json)

<!-- module-docs:start -->

Agent loop domain types.

These types define the core abstractions for the backend agentic loop.
They are pure domain primitives: no LLM backend references, no MCP types,
no infrastructure concerns.

# Modules

| Module | Contents |
|--------|----------|
| [`config`] | [`AgentConfig`] — loop control parameters |
| [`tool_types`] | [`ToolDefinition`], [`ToolCall`], [`ToolResult`] |
| [`messages`] | [`AgentMessage`] — closed conversation-turn enum |
| `messages_serde` | Custom `Serialize`/`Deserialize` impls for [`AssistantContent`] |
| [`events`] | [`AgentEvent`] (SSE units), [`LlmStreamEvent`] (stream protocol) |

# Design Principles

- [`AgentMessage`] is a closed enum so the type system prevents invalid states
  (e.g. a `User` message carrying `tool_calls`).
- [`ToolDefinition`] is a dedicated type — adapter layers convert `McpTool →
  ToolDefinition`; the agent domain must not depend on MCP domain types.
- [`ToolResult`] with `success: false` is **context for the LLM**, not an error;
  tool failures are fed back into the conversation so the model can reason about
  them and retry or adjust its approach.
- [`AgentEvent`] is the unit of SSE emission; every observable state change in
  the loop corresponds to exactly one variant.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-config-coverage.json) |
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-events-coverage.json) |
| [`messages.rs`](messages.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages-coverage.json) |
| [`messages_serde.rs`](messages_serde.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages_serde-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages_serde-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-messages_serde-coverage.json) |
| [`tool_display.rs`](tool_display.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_display-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_display-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_display-coverage.json) |
| [`tool_types.rs`](tool_types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-agent-tool_types-coverage.json) |
<!-- module-table:end -->

</details>
