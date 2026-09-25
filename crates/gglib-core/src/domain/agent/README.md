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
| [`loop_detection`] | [`LoopDetector`] — repeated tool-call-batch guard (FNV-1a batch signatures) |
| [`stagnation`] | [`StagnationDetector`] — repeated assistant-text guard |
| [`fnv1a`] | [`fnv1a::fnv1a_64`] — the hash backing both detectors |

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
