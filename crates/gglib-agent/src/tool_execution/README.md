# tool_execution

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-complexity.json)

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
- Each call is wrapped in a [`tokio::time::timeout`] capped at
  [`AgentConfig::tool_timeout_ms`].
- A timeout or `Err` from the executor produces a
  `ToolResult { success: false, … }` rather than aborting the batch —
  the LLM can observe the failure and decide how to proceed.
- [`AgentEvent::ToolCallStart`] and [`AgentEvent::ToolCallComplete`] are
  sent on `tx` before and after each call.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-tests-coverage.json) |
<!-- module-table:end -->

</details>
