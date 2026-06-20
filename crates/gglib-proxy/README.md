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
5. **Truncates oversized history** to protect local model context windows (see [History Truncation](#history-truncation))
6. **Exposes proxy telemetry** at `GET /v1/proxy/status` for CLI and web dashboards

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
| [`truncation.rs`](src/truncation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-coverage.json) |
| [`metrics.rs`](src/metrics.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-coverage.json) |
| [`mcp/`](src/mcp/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`server.rs`** — Axum application setup, routing, `/v1/chat/completions` and `/v1/proxy/status` handlers
- **`models.rs`** — `/v1/models` endpoint, OpenAI-compatible error response factories
- **`forward.rs`** — HTTP forwarding to llama-server with three-step request transform pipeline
- **`truncation.rs`** — Stateless history truncation pass (Step 3 of the request pipeline)
- **`metrics.rs`** — `ContextMetricsStore` ring buffer powering `/v1/proxy/status`
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

### Model Capability Auto-Detection

When the proxy auto-starts a llama-server for an incoming request, it reads the
model's capability tags and automatically enables the appropriate llama-server
flags — no extra configuration required:

| Tag | Effect |
|-----|--------|
| `"mtp"` | Enables MTP speculative decoding (`--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75`) |
| `"reasoning"` | Enables reasoning format extraction (`--reasoning-format deepseek` or equivalent) |
| `"agent"` | Enables Jinja template support (`--jinja`) |

Tags are detected automatically at model import time from GGUF metadata and
stored in the model catalog. This parity is architecturally enforced: the proxy,
GUI, and CLI all route through `build_server_config` in `gglib-runtime`, so any
model that works correctly when started from the GUI or CLI will behave
identically when auto-started by the proxy.

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
| 400 | Context window budget exceeded after truncation |
| 500 | Internal error |

## History Truncation

### Problem

Client-side context compaction can be broken for custom OpenAI-compatible
endpoints. When a local model calls tools, each tool response is permanently
embedded in the chat history by the client. After several tool-heavy turns the
prompt balloons past the local model's context window, causing it to fall into
repetition or logic loops.

### Defence

On every `/v1/chat/completions` request, the proxy applies a stateless
truncation pass **before** forwarding to llama-server:

| Constant | Value | Meaning |
|----------|-------|---------|
| `TOOL_CONTENT_THRESHOLD_CHARS` | **2,000** chars | Per-message content length that triggers replacement |
| `TOTAL_PAYLOAD_LIMIT_CHARS` | **240,000** chars | Total request body budget (≈ 60,000 tokens) |
| `PROTECTED_TAIL_COUNT` | **4** messages | Most-recent messages always preserved |

**Algorithm:**

1. Any unprotected `role: "tool"` or `role: "assistant"` message whose
   `content` string exceeds 2,000 characters has its content replaced with:

   > `[Raw tool output truncated by proxy to maintain context window. Rely on your previous observations.]`

2. `role: "system"` messages and the last 4 messages are never modified.
3. Array-form content (multi-part messages) and `tool_calls` fields are
   never touched.
4. If the total payload still exceeds 240,000 characters after step 1, the
   request is **rejected** with HTTP 400:
   ```json
   {
     "error": {
       "type": "context_length_exceeded",
       "code": "context_length_exceeded",
       "message": "Context window limit reached. Please start a new conversation."
     }
   }
   ```

**Zero blast radius:** On JSON parse failure the original body is forwarded
unchanged; the upstream llama-server produces its own diagnostic.

## Proxy Status Endpoint

```text
GET /v1/proxy/status
```

Returns a JSON snapshot of the last 20 requests processed by the truncation
pipeline. This is the shared data contract for the CLI TUI (future) and web
dashboard (future).

### Response shape

```json
{
  "total_requests": 42,
  "snapshots": [
    {
      "model_name": "qwen-3b",
      "payload_chars_before": 52000,
      "payload_chars_after": 8400,
      "messages_truncated": 3,
      "was_clamped": false,
      "recorded_at_secs": 1749283200
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `total_requests` | `u64` | All requests since proxy start, including evicted ones |
| `snapshots` | array | Last ≤ 20 requests, oldest-first |
| `payload_chars_before` | `usize` | Body size before truncation |
| `payload_chars_after` | `usize` | Body size after truncation |
| `messages_truncated` | `usize` | Number of messages whose content was replaced |
| `was_clamped` | `bool` | `true` when HTTP 400 was returned to the client |
| `recorded_at_secs` | `u64` | Unix timestamp of the observation |

The ring buffer retains at most 50 entries. `total_requests` grows
monotonically regardless of evictions.
