# handlers

HTTP request handlers for the gglib REST API.

## Purpose

This module contains all the Axum handler functions that implement the REST API endpoints for gglib. Each handler is responsible for:
- Extracting request data (path params, query params, JSON body)
- Validating inputs
- Calling appropriate service layer functions
- Formatting responses
- Error handling and HTTP status codes

## Architecture Pattern

All handlers follow a consistent pattern:
```rust
pub async fn handler_name(
    State(service): State<Arc<SomeService>>,
    extract: ExtractorType,
) -> Result<Json<ResponseType>, ApiError> {
    // 1. Extract and validate
    // 2. Call service layer
    // 3. Format response
    // 4. Return Result
}
```

## Handler Organization

### Model Management
- **`models.rs`** - List, add, remove models from catalog
- **`servers.rs`** - Start/stop llama-server processes

### Chat & Proxy
- **`chat.rs`** - Direct chat completions (legacy)
- **`proxy.rs`** - Proxy management and status

### Data Sources
- **`hf.rs`** - HuggingFace search and model discovery
- **`downloads.rs`** - Download queue management and progress

### Integration
- **`mcp.rs`** - Model Context Protocol server management
- **`settings.rs`** - Application settings CRUD
- **`events.rs`** - SSE event stream endpoint

### Agentic Loop
- **`agent/`** - Multi-turn SSE agentic loop endpoint (`POST /api/agent/chat`)
- **`port_utils.rs`** - Shared port validation utilities used across handlers

## Dependencies

All handlers depend on:
- **Service Layer**: `gglib_core::services::*` for business logic
- **Domain Types**: `gglib_core::domain::*` for models
- **DTOs**: `../dto/` for request/response serialization
- **Error Handling**: `../error.rs` for `ApiError` conversions

## Usage Example

```rust
use axum::{routing::get, Router};
use crate::handlers::models;

let app = Router::new()
    .route("/api/models", get(models::list_models))
    .route("/api/models", post(models::add_model))
    .with_state(app_state);
```

## Testing

Integration tests for handlers are in `tests/integration_*.rs` at the workspace root.

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`builtin.rs`](builtin.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-builtin-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-builtin-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-builtin-coverage.json) |
| [`chat.rs`](chat.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-chat-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-chat-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-chat-coverage.json) |
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-events-coverage.json) |
| [`mcp.rs`](mcp.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-mcp-coverage.json) |
| [`port_utils.rs`](port_utils.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-port_utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-port_utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-port_utils-coverage.json) |
| [`proxy.rs`](proxy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-proxy-coverage.json) |
| [`servers.rs`](servers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-servers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-servers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-servers-coverage.json) |
| [`voice.rs`](voice.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice-coverage.json) |
| [`voice_ws.rs`](voice_ws.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice_ws-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice_ws-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-handlers-voice_ws-coverage.json) |
| [`agent/`](agent/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-agent-coverage.json) |
| [`config/`](config/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-config-coverage.json) |
| [`model/`](model/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-model-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-model-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-model-coverage.json) |
<!-- module-table:end -->
