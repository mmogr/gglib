# mcp

<!-- module-docs:start -->

MCP (Model Context Protocol) server domain types.

These types represent MCP servers in the system, independent of any
infrastructure concerns (database, process management, etc.).

# Design

- `McpServer` - A persisted MCP server with ID
- `NewMcpServer` - An MCP server to be inserted (no ID yet)
- `McpServerConfig` - Execution configuration (`exe_path`, args, URL, `path_extra`)
- `McpServerType` - Connection type (stdio or SSE)
- `McpServerStatus` - Runtime status (stopped, starting, running, error)
- `McpLifecycle` - Startup lifecycle policy (eager, lazy, manual)
- `McpEnvEntry` - Environment variable entry
- `McpTool` - Tool exposed by an MCP server
- `McpToolResult` - Result of a tool invocation
- `ToolIndex` / `ToolSummary` - Progressive-disclosure tool registry index

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`tool_index.rs`](tool_index.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-tool_index-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-tool_index-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-tool_index-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-coverage.json) |
<!-- module-table:end -->

</details>
