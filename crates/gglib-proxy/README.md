# gglib-proxy

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json)

**Single Active Backend Proxy** - OpenAI-compatible proxy server for gglib.

## Architecture

This crate is in the **Infrastructure Layer** — it provides external API compatibility by bridging OpenAI clients to internal llama-server instances.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Infrastructure Layer                             │
│                                                                             │
│                              ┌──────────────────┐                           │
│                              │   gglib-proxy    │                           │
│                              │  OpenAI-compat   │                           │
│                              │   proxy server   │                           │
│                              └────────┬─────────┘                           │
│                                       │                                     │
│                                       │ (ports only, no infra deps)         │
│                                       │                                     │
└───────────────────────────────────────┼─────────────────────────────────────┘
                                        │
                                        ▼
                              ┌──────────────────┐
                              │    gglib-core    │
                              │   (port traits)  │
                              └──────────────────┘

At runtime, adapters inject concrete implementations:
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│gglib-runtime│     │  gglib-db   │     │  gglib-hf   │
│  (servers)  │     │   (models)  │     │  (search)   │
└─────────────┘     └─────────────┘     └─────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Overview

This crate provides an OpenAI-compatible HTTP server that:

1. **Receives requests** in OpenAI API format (`/v1/chat/completions`, `/v1/models`)
2. **Routes to llama-server** instances managed by gglib-runtime
3. **Streams responses** back to clients with proper SSE formatting

## Internal Structure

```text
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
│ OpenAI SDK  │────▶│ gglib-proxy │────▶│ llama-server     │
│ or Client   │◀────│ (this crate)│◀────│ (via runtime)    │
└─────────────┘     └─────────────┘     └──────────────────┘
```

### Key Design Decisions

- **Ports-only dependency**: Depends only on `gglib-core` (no sqlx, no gglib-runtime)
- **Bind externally**: `serve()` takes a pre-bound `TcpListener` from supervisor
- **Domain → API mapping**: OpenAI types live here, domain types in gglib-core

## Module Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-proxy                                          │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐      │
│  │  server.rs  │     │  models.rs  │     │ forward.rs  │     │   lib.rs    │      │
│  │   Axum app  │────▶│  /v1/models │────▶│  Streaming  │     │Entry point  │      │
│  │  & routing  │     │   endpoint  │     │   forward   │     │             │      │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘      │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`forward.rs`](src/forward.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-coverage.json) |
| [`server.rs`](src/server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`server.rs`** — Axum application setup, routing, `/v1/chat/completions` handler
- **`models.rs`** — `/v1/models` endpoint, model listing and resolution
- **`forward.rs`** — HTTP forwarding to llama-server with streaming support
- **`lib.rs`** — Public API and module re-exports


## API

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (always 200) |
| `/v1/models` | GET | List available models |
| `/v1/chat/completions` | POST | Chat completion (streaming/non-streaming) |

### Model Resolution

The `model` field in requests supports:
- Exact model name: `"llama-3.2-3b-q4_k_m"`
- Model ID: `"1"` (database ID)

### Context Size

Pass `num_ctx` at the request root to override default context size:

```json
{
  "model": "llama-3.2-3b-q4_k_m",
  "num_ctx": 8192,
  "messages": [...]
}
```

## Usage

This crate is used by `gglib-runtime`'s `ProxySupervisor`:

```rust
use async_trait::async_trait;
use gglib_core::ports::{
  CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
  ModelSummary, RunningTarget,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

fn main() -> anyhow::Result<()> {

#[derive(Debug)]
struct MockRuntimePort;

#[async_trait]
impl ModelRuntimePort for MockRuntimePort {
  async fn ensure_model_running(
    &self,
    _model_name: &str,
    _num_ctx: Option<u64>,
    _default_ctx: u64,
  ) -> Result<RunningTarget, ModelRuntimeError> {
    Ok(RunningTarget::local(
      12345,
      1,
      "mock-model".to_string(),
      4096,
    ))
  }

  async fn current_model(&self) -> Option<RunningTarget> {
    None
  }

  async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
    Ok(())
  }
}

#[derive(Debug)]
struct MockCatalogPort;

#[async_trait]
impl ModelCatalogPort for MockCatalogPort {
  async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
    Ok(vec![])
  }

  async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
    Ok(None)
  }

  async fn resolve_for_launch(
    &self,
    _name: &str,
  ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
    Ok(None)
  }
}

let rt = tokio::runtime::Runtime::new()?;
rt.block_on(async {
  // Supervisor binds the listener (use an ephemeral port in docs/tests)
  let listener = TcpListener::bind("127.0.0.1:0").await?;
  let addr = listener.local_addr()?;

  let runtime_port: Arc<dyn ModelRuntimePort> = Arc::new(MockRuntimePort);
  let catalog_port: Arc<dyn ModelCatalogPort> = Arc::new(MockCatalogPort);

  // Then calls serve with the listener
  let cancel = CancellationToken::new();
  let cancel_for_server = cancel.clone();

  let server = tokio::spawn(async move {
    gglib_proxy::serve(listener, 4096, runtime_port, catalog_port, cancel_for_server).await
  });

  // Optional: verify the server responds
  let health_url = format!("http://{addr}/health");
  for _ in 0..25 {
    if let Ok(resp) = reqwest::get(&health_url).await {
      if resp.status().is_success() {
        break;
      }
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
  }

  // Shutdown
  cancel.cancel();
  server.await??;
  Ok::<(), anyhow::Error>(())
})?;

Ok(())
}
```

## Streaming

SSE responses are forwarded with proper headers:
- `Content-Type: text/event-stream`
- `Cache-Control: no-cache`
- `Connection: keep-alive`

The proxy preserves upstream headers (minus hop-by-hop) and strips `Authorization`.

## Error Handling

| Status | Condition |
|--------|-----------|
| 503 | Model is loading (retry after) |
| 502 | Failed to connect to llama-server |
| 404 | Model not found |
| 500 | Internal error |
