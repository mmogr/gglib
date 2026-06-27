# mcp

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-complexity.json)

<!-- module-docs:start -->

MCP Streamable HTTP gateway for the proxy.

Implements the MCP Streamable HTTP transport (spec 2025-03-26) so
external clients like OpenWebUI can discover and invoke gglib's
MCP tools through the same proxy that serves OpenAI-compatible
chat completions.

# Module layout

| Module       | Responsibility                                      |
|--------------|-----------------------------------------------------|
| `types`      | JSON-RPC 2.0 and MCP wire types (serde structs)     |
| `session`    | `Mcp-Session-Id` tracking and validation            |
| `meta_tools` | Progressive-disclosure index + 3 meta-tool specs    |
| `handlers`   | Axum route handlers for POST/GET/DELETE `/mcp`      |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`handlers.rs`](handlers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-handlers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-handlers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-handlers-coverage.json) |
| [`meta_tools.rs`](meta_tools.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-meta_tools-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-meta_tools-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-meta_tools-coverage.json) |
| [`session.rs`](session.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-session-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-session-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-session-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-types-coverage.json) |
<!-- module-table:end -->

</details>
