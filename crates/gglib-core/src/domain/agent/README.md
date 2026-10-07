# agent

<!-- module-docs:start -->

Agent loop domain types.

These types define the core abstractions for the backend agentic loop.
They are pure domain primitives: no LLM backend references, no MCP types,
no infrastructure concerns.

# Modules

| Module | Contents |
|--------|----------|
| [`config`] | [`AgentConfig`] — loop control parameters |
| `limits` | [`TurnLimits`] — a turn's iteration and stagnation limits: what it names, then the stored settings, then the default; the one resolver the daemon and the CLI share |
| [`tool_types`] | [`ToolDefinition`], [`ToolCall`], [`ToolResult`] |
| [`messages`] | [`AgentMessage`] — closed conversation-turn enum; a user turn names its images by id, and each is charged [`IMAGE_CHARGE_CHARS`] against the context budget |
| `messages_serde` | Custom `Serialize`/`Deserialize` impls for [`AssistantContent`] |
| [`events`] | [`AgentEvent`] (SSE units), [`LlmStreamEvent`] (stream protocol) |
| [`loop_detection`] | [`LoopDetector`] — repeated tool-call-batch guard (FNV-1a batch signatures) |
| [`stagnation`] | [`StagnationDetector`] — repeated assistant-text guard |
| `transcript` | [`to_new_message`] — an agent message as a saved chat row; [`saved_history`] — a saved chat's prompt and rows as the messages its next turn starts from |
| `replay` | [`rows_from_frames`] — a reply's saved rows, rebuilt from its logged events, each turn's [`TurnUsage`] saved under [`MADE_KEYS`], its context's size, the messages trimmed and why it stopped among them |
| `turn_usage` | [`TurnUsage`] — how one model turn was made: model, token counts, times, why it stopped; never text. [`ContextReading`] — how large the answering server's context was and how many earlier messages did not fit, under one spelling for the proxy's usage frame and the `turn_usage` event |
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
