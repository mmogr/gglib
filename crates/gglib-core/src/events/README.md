# events

<!-- module-docs:start -->

Canonical event union for all cross-adapter events.

This module is the single source of truth for events used by SSE handlers and
backend emitters. There is no Tauri-event branch: the desktop app consumes the
same `/api/events` stream a browser tab does.

# Structure

- `app` - Application-level events (model added/removed/updated)
- `server` - Model server lifecycle events

Download events are not a submodule here: `AppEvent::Download` wraps
`crate::download::DownloadEvent` verbatim.

# Wire Format

Events are serialized with a `type` tag for TypeScript compatibility:

```json
{ "type": "server_started", "modelName": "Llama-2-7B", "port": 8080 }
```

`server_error` carries a structured error envelope (message, stable `type`
discriminant, `retryable` flag) rather than a bare string, mirroring the HTTP
layer's `ErrorResponse` shape so SSE and HTTP clients agree on meaning:

```json
{
  "type": "server_error",
  "modelId": 1,
  "modelName": "Llama-2-7B",
  "error": { "message": "...", "type": "service_unavailable", "retryable": true }
}
```

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`app.rs`](app.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app-coverage.json) |
| [`app_event_tests.rs`](app_event_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app_event_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app_event_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-app_event_tests-coverage.json) |
| [`remote.rs`](remote.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-remote-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-remote-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-remote-coverage.json) |
| [`server.rs`](server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-server-coverage.json) |
<!-- module-table:end -->

</details>
