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
- **`chat_proxy.rs`** - OpenAI-compatible proxy chat endpoint
- **`proxy.rs`** - Proxy management and status

### Data Sources
- **`hf.rs`** - HuggingFace search and model discovery
- **`downloads.rs`** - Download queue management and progress

### Integration
- **`mcp.rs`** - Model Context Protocol server management
- **`settings.rs`** - Application settings CRUD
- **`events.rs`** - SSE event stream endpoint

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
