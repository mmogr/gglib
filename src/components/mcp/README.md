# mcp

<!-- module-docs:start -->

Controls for tool servers (MCP) that live outside the panel's own file.

| File | Role |
|------|------|
| `AllowRemoteCheckbox.tsx` | Whether paired devices may use one tool server: one box for the server's own flag, set through its own route and read by a device's next run ([ADR 0025](../../../docs/adr/0025-a-server-is-opened-to-paired-devices-by-a-flag-on-it.md)). A disabled server is said to offer nothing, and a refused change is said beside the box. `McpServersPanel` shows one on each row it can open, the one place the flag is set |

<!-- module-docs:end -->
