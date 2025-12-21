# http

<!-- module-docs:start -->

HTTP route constants for the REST API.

These constants define the canonical paths for all HTTP endpoints, ensuring consistency between `gglib-axum` routes and any client code that constructs URLs.

## Example Routes

| Constant | Value | Description |
|----------|-------|-------------|
| `MODELS` | `/api/models` | Model CRUD operations |
| `CHAT_COMPLETIONS` | `/api/chat/completions` | OpenAI-compatible chat |
| `MCP_SERVERS` | `/api/mcp/servers` | MCP server management |
| `HEALTH` | `/health` | Server health check |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`hf.rs`](hf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-hf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-hf-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-http-hf-coverage.json) |
<!-- module-table:end -->

</details>
