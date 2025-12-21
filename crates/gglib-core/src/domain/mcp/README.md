# mcp

<!-- module-docs:start -->

MCP (Model Context Protocol) server domain types.

These types represent MCP server configurations and runtime state, shared between the Rust backend and TypeScript frontend.

## Key Types

| Type | Description |
|------|-------------|
| `McpServer` | A persisted MCP server with ID, name, and configuration |
| `NewMcpServer` | Data for registering a new MCP server |
| `McpServerConfig` | Execution config (command, args, URL, working directory) |
| `McpServerType` | Connection type: `Stdio` (gglib spawns) or `Sse` (external HTTP) |
| `McpServerStatus` | Runtime status: `Stopped`, `Starting`, `Running`, `Error` |
| `McpTool` | A tool exposed by an MCP server |
| `McpEnvEntry` | Environment variable key-value pair |

## Connection Types

```text
┌────────────────────────────────────────┐
│            Stdio Server                │
│  gglib spawns & manages the process    │
│  Communication via stdin/stdout        │
│  Example: npx @modelcontextprotocol/X  │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│            SSE Server                  │
│  External process (user-managed)       │
│  gglib connects via HTTP SSE           │
│  Example: http://localhost:3001/sse    │
└────────────────────────────────────────┘
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-types-coverage.json) |
<!-- module-table:end -->

</details>
