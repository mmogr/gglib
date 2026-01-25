# gglib-proxy

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json)

**Single Active Backend Proxy** - OpenAI-compatible proxy server for gglib.

## Architecture

This crate is in the **Infrastructure Layer** — it provides external API compatibility by bridging OpenAI clients to internal llama-server instances.

```text
                              ┌──────────────────┐
                              │   gglib-proxy    │
                              │  OpenAI-compat   │
                              │   proxy server   │
                              └────────┬─────────┘
                                       │
         ┌─────────────┬───────────────┼───────────────┬─────────────┐
         ▼             ▼               ▼               ▼             ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │  gglib-hf   │ │gglib-runtime│ │gglib-download│ │  gglib-mcp  │
│   SQLite    │ │  HF client  │ │   Servers   │ │  Downloads  │ │ MCP servers │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
         │             │               │               │             │
         └─────────────┴───────────────┴───────────────┴─────────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    gglib-core    │
                              │   (all ports)    │
                              └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Overview

This crate provides an OpenAI-compatible HTTP server that:

1. **Receives requests** in OpenAI API format (`/v1/chat/completions`, `/v1/models`)
2. **Routes to llama-server** instances managed by gglib-runtime
3. **Streams responses** back to clients with proper SSE formatting

## Internal Structure

```
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
| [`forward.rs`](src/forward) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-coverage.json) |
| [`models.rs`](src/models) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-coverage.json) |
| [`server.rs`](src/server) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-coverage.json) |
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
// Supervisor binds the listener
let listener = TcpListener::bind("127.0.0.1:11444").await?;

// Then calls serve with the listener
gglib_proxy::serve(
    listener,
    default_ctx,
    runtime_port,    // Arc<dyn ModelRuntimePort>
    catalog_port,    // Arc<dyn ModelCatalogPort>
    cancel_token,
).await?;
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
