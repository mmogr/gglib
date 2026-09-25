# builtin

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-builtin-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-builtin-complexity.json)

<!-- module-docs:start -->

In-process built-in tool executor.

Implements [`ToolExecutorPort`] for tools that run directly inside the
server process rather than through an external MCP child process.

# Tool-name format

Names are qualified with `"builtin:"` (e.g. `"builtin:get_current_time"`),
matching the convention used by [`crate::tool_executor::McpToolExecutorAdapter`] where names
are qualified with the numeric server id (e.g. `"3:read_file"`).
[`crate::CombinedToolExecutor`] routes calls with the `"builtin:"` prefix here.

<!-- module-docs:end -->
