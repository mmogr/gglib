# Tool Executor Filter

<!-- module-docs:start -->

[`FilteredToolExecutor`] and [`EmptyToolExecutor`] — decorators that
restrict a [`ToolExecutorPort`] to a named allowlist of tools.

# Architectural placement

These decorators live in `gglib-core::ports` because they depend only on the
[`ToolExecutorPort`] trait and domain types (`ToolCall`, `ToolDefinition`,
`ToolResult`) — all of which are defined here.  Placing them in `gglib-core`
makes them available to any adapter crate without introducing an additional
dependency on `gglib-agent`.

# Security model

The allowlist is enforced on **both** `list_tools` (so the LLM only sees
permitted tools) and `execute` (so an adversarially-prompted model that
synthesises a call for a tool it was never told about cannot bypass the
filter).

[`ToolExecutorPort`]: crate::ports::ToolExecutorPort

<!-- module-docs:end -->
