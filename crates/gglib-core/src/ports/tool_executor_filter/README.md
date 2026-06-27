# tool_executor_filter

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-tool_executor_filter-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-tool_executor_filter-complexity.json)

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

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`empty.rs`](empty.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-empty-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-empty-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-empty-coverage.json) |
| [`filtered.rs`](filtered.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-filtered-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-filtered-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-filtered-coverage.json) |
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tool_executor_filter-tests-coverage.json) |
<!-- module-table:end -->

</details>
