# events

<!-- module-docs:start -->

Canonical event union for all cross-adapter events.

This module is the single source of truth for events used by Tauri listeners,
SSE handlers, and backend emitters.

# Structure

- `app` - Application-level events (model added/removed/updated)
- `download` - Download progress and completion events
- `server` - Model server lifecycle events
- `mcp` - MCP server lifecycle events

# Wire Format

Events are serialized with a `type` tag for TypeScript compatibility:

```json
{ "type": "server_started", "modelName": "Llama-2-7B", "port": 8080 }
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`app.rs`](app.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-coverage.json) |
| [`download.rs`](download.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-coverage.json) |
| [`mcp.rs`](mcp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-coverage.json) |
| [`server.rs`](server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-coverage.json) |
<!-- module-table:end -->

</details>
