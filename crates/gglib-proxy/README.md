# gglib-proxy

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json)

**Dual API Compatibility Proxy** - OpenAI and Ollama compatible proxy server for gglib.

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

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Overview

This crate provides a dual-API HTTP proxy server that supports both OpenAI and Ollama clients simultaneously:

1. **OpenAI API** (`/v1/*`) — Original compatibility layer for OpenAI SDK clients
2. **Ollama API** (`/api/*`) — Native Ollama endpoints, making gglib a drop-in replacement for Ollama
3. **Format translation** — Ollama requests ↔ OpenAI format ↔ llama-server ↔ response translation
4. **Streaming adaptation** — SSE (Server-Sent Events) ↔ NDJSON (Newline-Delimited JSON)

## Internal Structure

```text
┌─────────────┐     ┌─────────────────────────────┐     ┌──────────────────┐
│ OpenAI SDK  │────▶│ /v1/* (OpenAI endpoints)     │────▶│                  │
│             │     │                             │     │  llama-server    │
└─────────────┘     │      gglib-proxy            │     │  (via runtime)   │
                    │    (this crate)             │     │                  │
┌─────────────┐     │                             │     │                  │
│Ollama Client│────▶│ /api/* (Ollama endpoints)   │────▶│                  │
│             │◀────│  + SSE↔NDJSON translation   │◀────│                  │
└─────────────┘     └─────────────────────────────┘     └──────────────────┘
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
│  OpenAI API (original)                    Ollama API (new)                          │
│  ┌─────────────┐     ┌─────────────┐     ┌──────────────────┐                      │
│  │  server.rs  │     │  models.rs  │     │ ollama_models.rs │                      │
│  │   Axum app  │────▶│  /v1/models │     │  Ollama types +  │                      │
│  │  & routing  │     │   endpoint  │     │  normalization   │                      │
│  └──────┬──────┘     └─────────────┘     └──────────────────┘                      │
│         │                                                                           │
│         │            ┌─────────────┐     ┌──────────────────┐                      │
│         └───────────▶│ forward.rs  │     │ollama_handlers.rs│                      │
│                      │  Streaming  │     │ 13 Ollama routes │                      │
│                      │   forward   │     │  + translation   │                      │
│                      └─────────────┘     └────────┬─────────┘                      │
│                                                   │                                 │
│                                          ┌────────▼──────────┐                      │
│                                          │ollama_stream.rs   │                      │
│                                          │ SSE↔NDJSON adapter│                      │
│                                          └───────────────────┘                      │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`forward.rs`](src/forward.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-coverage.json) |
| [`ollama_handlers.rs`](src/ollama_handlers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_handlers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_handlers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_handlers-coverage.json) |
| [`ollama_models.rs`](src/ollama_models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_models-coverage.json) |
| [`ollama_stream.rs`](src/ollama_stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-ollama_stream-coverage.json) |
| [`server.rs`](src/server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**

**OpenAI API (original):**
- **`server.rs`** — Axum application setup, unified routing for both APIs, shared ProxyState
- **`models.rs`** — `/v1/models` endpoint, model listing and resolution
- **`forward.rs`** — HTTP forwarding to llama-server with SSE streaming support
- **`lib.rs`** — Public API and module re-exports

**Ollama API (new):**
- **`ollama_models.rs`** — Ollama data types, model name normalization (`:latest` stripping), timestamp helpers
- **`ollama_handlers.rs`** — 13 Ollama route handlers (`/api/chat`, `/api/generate`, `/api/tags`, etc.), format translation
- **`ollama_stream.rs`** — SSE↔NDJSON streaming adapter using `futures_util::stream::unfold`


## API

### Endpoints

**Health & Common:**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (always 200) |

**OpenAI API (original):**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/models` | GET | List available models |
| `/v1/chat/completions` | POST | Chat completion (streaming/non-streaming) |

**Ollama API (new):**
| Endpoint | Method | Description |
|----------|--------|-------------|
| `GET /` | GET | Root probe ("Ollama is running") |
| `/api/version` | GET | Version info |
| `/api/tags` | GET | List models (Ollama format) |
| `/api/show` | POST | Model metadata |
| `/api/ps` | GET | Running models |
| `/api/chat` | POST | Chat completions (streaming/non-streaming, NDJSON) |
| `/api/generate` | POST | Text generation (streaming/non-streaming, NDJSON) |
| `/api/embed` | POST | Generate embeddings |
| `/api/embeddings` | POST | Legacy single-embedding endpoint |
| `/api/pull` | POST | Stub (redirects to CLI) |
| `/api/delete` | DELETE | Stub (redirects to CLI) |
| `/api/copy` | POST | Stub (not supported) |
| `/api/create` | POST | Stub (not supported) |

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

This crate is used by `gglib-runtime`'s `ProxySupervisor`. The supervisor binds a `TcpListener` and passes it to `gglib_proxy::serve()` along with port trait implementations:

```rust,ignore
use gglib_proxy;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

let listener = TcpListener::bind("127.0.0.1:8080").await?;
let cancel = CancellationToken::new();

gglib_proxy::serve(
    listener,
    4096,                    // default context size
    runtime_port,            // Arc<dyn ModelRuntimePort>
    catalog_port,            // Arc<dyn ModelCatalogPort>
    cancel,
).await?;
```

See the [full doctest](src/lib.rs) for a complete example with mock implementations.

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
