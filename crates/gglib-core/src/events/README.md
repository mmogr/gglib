# events

<!-- module-docs:start -->

Canonical event types for cross-adapter communication.

This module defines the unified event system used by Tauri listeners, SSE handlers, and backend emitters. All events are serializable with a `type` tag for TypeScript compatibility.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              events/                                                │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│                    ┌──────────────────────────────────────┐                         │
│                    │          AppEvent (enum)             │                         │
│                    │  Discriminated union of all events   │                         │
│                    └───────────────┬──────────────────────┘                         │
│                                    │                                                │
│      ┌────────────┬────────────────┼────────────────┬────────────┐                  │
│      ▼            ▼                ▼                ▼            ▼                  │
│  ┌────────┐  ┌────────┐      ┌────────────┐   ┌────────┐   ┌────────┐               │
│  │  app   │  │download│      │   server   │   │  mcp   │   │  ...   │               │
│  │ events │  │ events │      │   events   │   │ events │   │        │               │
│  └────────┘  └────────┘      └────────────┘   └────────┘   └────────┘               │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Event Categories

| Category | Examples |
|----------|----------|
| `app` | `ModelAdded`, `ModelRemoved`, `ModelUpdated` |
| `download` | `DownloadProgress`, `DownloadComplete`, `DownloadError` |
| `server` | `ServerStarted`, `ServerStopped`, `ServerLogLine` |
| `mcp` | `McpServerStarted`, `McpServerError`, `McpToolsUpdated` |

## Wire Format

```json
{ "type": "server_started", "modelName": "Llama-2-7B", "port": 8080 }
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`app.rs`](app) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-coverage.json) |
| [`download.rs`](download) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-download-coverage.json) |
| [`mcp.rs`](mcp) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-mcp-coverage.json) |
| [`server.rs`](server) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-coverage.json) |
<!-- module-table:end -->

</details>
