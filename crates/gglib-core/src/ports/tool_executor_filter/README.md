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

The allowlist is enforced on `list_tools` (so the LLM only sees permitted
tools) and on **both** `execute` and `execute_with_progress` (so an
adversarially-prompted model that synthesises a call for a tool it was never
told about cannot bypass the filter; the agent loop calls the progress form,
so a filter that checked only `execute` would check nothing).

[`ToolExecutorPort`]: crate::ports::ToolExecutorPort

<!-- module-docs:end -->
