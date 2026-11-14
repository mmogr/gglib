# mcp

<!-- module-docs:start -->

MCP (Model Context Protocol) server domain types.

These types represent MCP servers in the system, independent of any
infrastructure concerns (database, process management, etc.).

# Design

- `McpServer` - A persisted MCP server with ID
- `NewMcpServer` - An MCP server to be inserted (no ID yet)
- `McpServerConfig` - Execution configuration (`exe_path`, args, `path_extra`)
- `McpServerStatus` - Runtime status (stopped, starting, running, error)
- `McpLifecycle` - Startup lifecycle policy (eager, lazy)
- `McpEnvEntry` - Environment variable entry
- `McpTool` - Tool exposed by an MCP server
- `McpToolResult` - Result of a tool invocation
- `DeviceServers` - Which of the servers open to paired devices a device's run may use: all, or only those it names by id; none by default
- `ToolIndex` / `ToolSummary` - Progressive-disclosure tool registry index

<!-- module-docs:end -->
