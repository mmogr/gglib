# Tool Execution

<!-- module-docs:start -->

Parallel tool execution with bounded concurrency and per-tool timeout.

# Behaviour

- All tool calls in a batch are dispatched concurrently via a
  [`tokio::task::JoinSet`].  When the future returned by
  [`execute_tools_parallel`] is dropped (e.g. because `AgentTaskGuard`
  aborts the parent agent task on client disconnect), the `JoinSet` is
  dropped and every in-flight sub-task is cancelled — no resource leak.
- A [`tokio::sync::Semaphore`] caps the number of *simultaneously running*
  tool calls at [`AgentConfig::max_parallel_tools`].
- Each call is wrapped in a [`tokio::time::timeout`]: the tool's own
  [`ToolDefinition::deadline`] when it declares one (an image render takes
  minutes; a request cannot set it, so the request clamp on
  [`AgentConfig::tool_timeout_ms`] does not apply), else
  [`AgentConfig::tool_timeout_ms`].
- The call runs through `ToolExecutorPort::execute_with_progress`. What the
  tool reports becomes `AgentEvent::ToolProgress` (a stage change, a new
  pass or a pass's last step at once, anything else at most once a second)
  and each preview frame an `AgentEvent::ToolPreview`, never logged. Both
  are sent with `try_send`: a full channel drops them rather than slowing
  the tool.
- A timeout or `Err` from the executor produces a
  `ToolResult { success: false, … }` rather than aborting the batch —
  the LLM can observe the failure and decide how to proceed.
- [`AgentEvent::ToolCallStart`] and [`AgentEvent::ToolCallComplete`] are
  sent on `tx` before and after each call.

<!-- module-docs:end -->
