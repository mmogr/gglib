# gglib-proxy

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json)

**Single Active Backend Proxy** - OpenAI-compatible proxy server for gglib with an integrated MCP Streamable HTTP gateway.

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

This crate provides an OpenAI-compatible HTTP server that:

1. **Receives requests** in OpenAI API format (`/v1/chat/completions`, `/v1/models`)
2. **Routes to llama-server** instances managed by gglib-runtime
3. **Streams responses** back to clients with proper SSE formatting
4. **Exposes MCP tools** via [MCP Streamable HTTP](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#streamable-http) at `/mcp`

## Internal Structure

```text
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
│ OpenAI SDK  │────▶│ gglib-proxy │────▶│ llama-server     │
│ or Client   │◀────│ (this crate)│◀────│ (via runtime)    │
└─────────────┘     └──────┬──────┘     └──────────────────┘
                           │
                    POST /mcp (JSON-RPC)
                           │
               ┌───────────▼────────────┐
               │   MCP servers (via     │
               │   gglib-mcp service)   │
               └────────────────────────┘
```

### Key Design Decisions

- **Ports-only dependency**: Depends only on `gglib-core` (no sqlx, no gglib-runtime)
- **Bind externally**: `serve()` takes a pre-bound `TcpListener` from supervisor
- **Router, not validator**: Inbound `/v1/chat/completions` requests are parsed into a narrow `ChatRoutingEnvelope` (just `model`, `stream`, `num_ctx`) and then forwarded as raw bytes. Unknown fields and OpenAI content variants (array-form `content`, bare-string `stop`, future extensions) pass through unchanged. Schema validation is llama-server's responsibility.
- **Domain → API mapping**: OpenAI types live here, domain types in gglib-core

## Module Architecture

```text
┌──────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-proxy                                          │
├──────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                │
│  │  server.rs  │  │  models.rs  │  │ forward.rs  │  │   lib.rs    │                │
│  │   Axum app  │─▶│  /v1/models │─▶│  Streaming  │  │Entry point  │                │
│  │  & routing  │  │   endpoint  │  │   forward   │  │             │                │
│  └──────┬──────┘  └─────────────┘  └─────────────┘  └─────────────┘                │
│         │                                                                            │
│         │ /mcp                                                                       │
│         ▼                                                                            │
│  ┌─────────────────────────────────────────────┐                                     │
│  │  mcp/                                       │                                     │
│  │  ├─ handlers.rs  (POST/GET/DELETE /mcp)     │                                     │
│  │  ├─ types.rs     (JSON-RPC & MCP wire types)│                                     │
│  │  └─ session.rs   (Mcp-Session-Id tracking)  │                                     │
│  └─────────────────────────────────────────────┘                                     │
│                                                                                      │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`forward.rs`](src/forward.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-coverage.json) |
| [`server.rs`](src/server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-coverage.json) |
| [`mcp/`](src/mcp/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`server.rs`** — Axum application setup, routing, `/v1/chat/completions` handler
- **`models.rs`** — `/v1/models` endpoint, model listing and resolution
- **`forward.rs`** — HTTP forwarding to llama-server with streaming support
- **`mcp/`** — MCP Streamable HTTP gateway (see [below](#mcp-streamable-http-gateway))
  - **`mcp/handlers.rs`** — `POST /mcp` JSON-RPC dispatch, `GET /mcp` (405), `DELETE /mcp` (terminate session)
  - **`mcp/types.rs`** — JSON-RPC 2.0 and MCP protocol wire types
  - **`mcp/session.rs`** — `Mcp-Session-Id` tracking and validation
- **`lib.rs`** — Public API and module re-exports


## API

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (always 200) |
| `/v1/models` | GET | List available models |
| `/v1/chat/completions` | POST | Chat completion (streaming/non-streaming) |
| `/mcp` | POST | MCP Streamable HTTP — JSON-RPC dispatch |
| `/mcp` | GET | Returns 405 (server-push not yet supported) |
| `/mcp` | DELETE | Terminate MCP session by `Mcp-Session-Id` |

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
    mcp,                     // Arc<McpService>
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
## MCP Streamable HTTP Gateway

The proxy includes a built-in [MCP Streamable HTTP](https://modelcontextprotocol.io/specification/2025-03-26/basic/transports#streamable-http) gateway at `/mcp`. This lets any MCP-compatible client (including OpenWebUI) discover and invoke tools from gglib's configured MCP servers — no separate `mcpo` process or Python dependency required.

### How it works

1. Client sends `POST /mcp` with a JSON-RPC `initialize` request
2. Server returns capabilities and an `Mcp-Session-Id` header
3. Client sends `notifications/initialized` to confirm
4. Client calls `tools/list` to discover available tools
5. Client calls `tools/call` to invoke a tool (response is SSE)
6. Client sends `DELETE /mcp` when done

Tool names are qualified as `{server_name}__{tool_name}` so tools from different MCP servers never collide.

### Configuring OpenWebUI

When running the proxy (e.g. `gglib proxy --port 8080`), configure OpenWebUI with:

| Setting | Value |
|---------|-------|
| **OpenAI API Base URL** | `http://localhost:8080/v1` |
| **MCP Server URL** | `http://localhost:8080/mcp` |

Both chat completions and MCP tools are served from the same proxy — a single connection point for all gglib capabilities.

### Testing with curl

```bash
# Initialize a session
curl -X POST http://localhost:8080/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"1.0"}}}'

# List tools (use the Mcp-Session-Id from the response above)
curl -X POST http://localhost:8080/mcp \
  -H 'Content-Type: application/json' \
  -H 'Mcp-Session-Id: <session-id>' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```
## Error Handling

| Status | Condition |
|--------|-----------|
| 503 | Model is loading (retry after) |
| 502 | Failed to connect to llama-server |
| 404 | Model not found |
| 500 | Internal error |
