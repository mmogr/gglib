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
6. **Exposes a live proxy dashboard** — active connections, per-slot context usage, and recent request history — via `GET /v1/proxy/status` (JSON) and `GET /v1/proxy/status/stream` (SSE), consumed by both the CLI (`gglib proxy dashboard`) and the web GUI's Proxy Dashboard modal (see [Proxy Dashboard](#proxy-dashboard))

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
│         │ GET /v1/proxy/status(/stream)                                             │
│         ▼                                                                            │
│  ┌─────────────────────────────────────────────┐                                     │
│  │  dashboard.rs                                │                                     │
│  │  aggregates connections.rs (active reqs),    │                                     │
│  │  slots.rs / slots_poller.rs (context usage), │                                     │
│  │  metrics.rs (recent requests) into one       │                                     │
│  │  DashboardSnapshot (JSON or SSE)              │                                     │
│  └─────────────────────────────────────────────┘                                     │
│                                                                                      │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`cache_lifecycle.rs`](src/cache_lifecycle.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_lifecycle-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_lifecycle-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_lifecycle-coverage.json) |
| [`cache_metrics.rs`](src/cache_metrics.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_metrics-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_metrics-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-cache_metrics-coverage.json) |
| [`canonicalization.rs`](src/canonicalization.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-canonicalization-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-canonicalization-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-canonicalization-coverage.json) |
| [`connections.rs`](src/connections.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-connections-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-connections-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-connections-coverage.json) |
| [`council_proxy.rs`](src/council_proxy.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-council_proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-council_proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-council_proxy-coverage.json) |
| [`dashboard.rs`](src/dashboard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-dashboard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-dashboard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-dashboard-coverage.json) |
| [`forward.rs`](src/forward.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-forward-coverage.json) |
| [`metrics.rs`](src/metrics.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-metrics-coverage.json) |
| [`models.rs`](src/models.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models-coverage.json) |
| [`models_tests.rs`](src/models_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-models_tests-coverage.json) |
| [`server.rs`](src/server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-server-coverage.json) |
| [`slot_eviction.rs`](src/slot_eviction.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slot_eviction-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slot_eviction-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slot_eviction-coverage.json) |
| [`slots.rs`](src/slots.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots-coverage.json) |
| [`slots_poller.rs`](src/slots_poller.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots_poller-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots_poller-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-slots_poller-coverage.json) |
| [`sse_stream.rs`](src/sse_stream.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-sse_stream-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-sse_stream-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-sse_stream-coverage.json) |
| [`token_calibration.rs`](src/token_calibration.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-token_calibration-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-token_calibration-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-token_calibration-coverage.json) |
| [`truncation.rs`](src/truncation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-truncation-coverage.json) |
| [`upstream_health.rs`](src/upstream_health.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-upstream_health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-upstream_health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-upstream_health-coverage.json) |
| [`mcp/`](src/mcp/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-mcp-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`server.rs`** — Axum application setup, routing, `/v1/chat/completions`, `/v1/proxy/status`, and `/v1/proxy/status/stream` handlers
- **`models.rs`** — `/v1/models` endpoint, OpenAI-compatible error response factories
- **`forward.rs`** — HTTP forwarding to llama-server with three-step request transform pipeline
- **`truncation.rs`** — Stateless history truncation pass (Step 3 of the request pipeline)
- **`token_calibration.rs`** — Per-model chars-per-token estimator (EWMA over real `usage.prompt_tokens`) that sizes the truncation budget
- **`upstream_health.rs`** — Consecutive-failure watchdog that recycles a degraded (empty-response / first-byte-timeout) llama-server; feeds `DashboardSnapshot.upstream_health`
- **`metrics.rs`** — `ContextMetricsStore` ring buffer feeding `DashboardSnapshot.recent_requests`
- **`connections.rs`** — `ActiveConnectionsRegistry` + RAII `ConnectionGuard`; tracks every in-flight `/v1/chat/completions` request (direct and council/virtual-model) through `Queued` → `ProcessingPrompt` → `Generating`, feeding `DashboardSnapshot.active_connections`
- **`slots.rs`** — Fetch + defensive parsing of llama.cpp's native `GET /slots` endpoint into `SlotSnapshot`; also provides slot I/O primitives (`save_slot`, `restore_slot`, `clear_slot_files`, `sanitize_session_id`) and background LRU eviction
- **`canonicalization.rs`** — System prompt normalization for cache key stability (whitespace, tool definition ordering)
- **`cache_lifecycle.rs`** — KV cache save→forward→save orchestration with semaphore gating and retry logic
- **`sse_stream.rs`** — SSE stream extraction helper for separating chat completion responses from Server-Sent Events
- **`slots_poller.rs`** — Background task that polls `slots.rs` on an interval with exponential backoff, caching the latest `SlotsPollResult`
- **`dashboard.rs`** — `DashboardSnapshot`, the unified data contract aggregating `connections.rs` + `slots_poller.rs` + `metrics.rs`; `spawn_dashboard_publisher` recomputes and broadcasts it once per second for `/v1/proxy/status/stream` subscribers
- **`council_proxy.rs`** — Routes virtual-model (council/orchestrator) requests; registers active connections and forwards `AgentEvent::PromptProgress` the same way `forward.rs` does for direct completions
- **`mcp/`** — MCP Streamable HTTP gateway (see [below](#mcp-streamable-http-gateway))
  - **`mcp/handlers.rs`** — `POST /mcp` JSON-RPC dispatch, `GET /mcp` (405), `DELETE /mcp` (terminate session)
  - **`mcp/types.rs`** — JSON-RPC 2.0 and MCP protocol wire types
  - **`mcp/session.rs`** — `Mcp-Session-Id` tracking and validation
- **`lib.rs`** — Public API and module re-exports


## KV Cache Session Persistence

When enabled via `--cache` and `--slot-dir`, the proxy saves per-session KV cache
state to disk between requests, enabling sequential multi-agent workflows to
resume from prior context without re-computation.

- **Flat on-disk layout:** slot files are stored directly under `--slot-dir` as
  `{model_id}__{session_id}.bin` (no per-session subdirectories — llama-server's
  save/restore endpoint rejects filenames containing a path separator).
- **Atomic saves:** a save asks llama-server to write to a per-attempt temp name
  (`{model_id}__{session_id}.{nonce}.tmp`) and only renames it onto the final
  `.bin` name after a confirmed-complete write. Restore/eviction only ever see
  `*.bin`, so a save that times out or is retried mid-write can never produce a
  torn or partially-written file at the name anything else reads. Save/restore
  use generous timeouts (120s/60s) matching multi-GB slot files.
- **Semaphore gating:** A `Semaphore(1)` ensures only one save→forward→save cycle
  runs at a time (single-slot llama-server constraint).
- **Fail-open mtime guard:** If a cached slot file predates the current
  llama-server process's start time (indicating a stale cache from a prior
  server instance), restore is skipped.
- **Partial-KV models bypass the layer entirely:** sliding-window, hybrid, and
  recurrent/SSM architectures keep only part of the token history in KV memory.
  llama-server's slot files omit the context checkpoints those models need to
  resume, so a restore re-prefills the whole prompt — and, by pre-filling the
  slot, stops llama-server from consulting its host-RAM prompt cache, which
  *does* carry checkpoints and would have resumed cheaply. The proxy therefore
  makes no save or restore calls for these models and leaves conversation
  switching to the RAM cache. Detected from GGUF metadata by
  `gglib_core::domain::kv_memory_is_partial`, resolved per launch by
  `gglib_runtime::llama::args::resolve_slot_restore` (which logs its reasoning),
  and overridable with `GGLIB_FORCE_HYBRID_DISK_CACHE=1`.
- **Disk-aware byte-budget eviction:** a background sweep evicts the
  least-recently-used `.bin` files once the cache exceeds a byte budget — by
  default a quarter of (free disk space + the cache's own footprint),
  recomputed on every sweep; override with `--cache-disk-gb` or
  `GGLIB_CACHE_DISK_GB`. The same sweep also reaps orphaned `.tmp` files left
  behind by an interrupted save.
- **Clear endpoint:** `POST /v1/proxy/cache/clear` (with optional `X-Gglib-Session-Id`
  header) clears cached slot files for a session or all sessions.

CLI usage:
```bash
gglib proxy start --cache --slot-dir ~/.cache/gglib/slots --cache-disk-gb 100
gglib proxy cache-clear --host 127.0.0.1 --port 8080 --session-id my-session
```

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
| `/v1/proxy/status` | GET | Proxy dashboard snapshot (JSON) — see [Proxy Dashboard](#proxy-dashboard) |
| `/v1/proxy/status/stream` | GET | Proxy dashboard live updates (SSE, hydrate-then-stream) |
| `/v1/proxy/cache/clear` | POST | Clear KV cache for a session or all sessions (optional `X-Gglib-Session-Id` header) |

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

### Inference Defaults Auto-Injection

On every `POST /v1/chat/completions` request the proxy resolves sampling
parameters through the same 4-level hierarchy used by every other surface:

```text
request params  →  model defaults  →  global settings  →  hardcoded fallback
```

1. **Request params** — whatever `temperature`, `top_p`, `top_k`, `max_tokens`,
   `repeat_penalty`, `presence_penalty`, and `min_p` the client sent.
2. **Model defaults** — per-model `inference_defaults` stored in the model
   catalog (`ModelSummary::inference_defaults`).
3. **Global settings** — `Settings::inference_defaults` loaded from the
   settings repository on every request.
4. **Hardcoded fallback** — `temperature=0.7`, `top_p=0.95`, `top_k=40`,
   `max_tokens=2048`, `repeat_penalty=1.0`, `presence_penalty=0.0`, `min_p=0.0`.

The resolved values are aggressively written into the forwarded request body
(via `body_obj.insert`) so llama-server always receives fully-specified
parameters rather than relying on its own defaults.  Client-supplied values
are always preserved because they form the base of the resolution hierarchy.

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
| `TOOL_CONTENT_THRESHOLD_CHARS` | **2,000** chars | Minimum per-message content length eligible for replacement |
| `TOTAL_PAYLOAD_LIMIT_CHARS` | **240,000** chars | Floor for the request body budget (≈ 60,000 tokens) |
| `PROTECTED_TAIL_COUNT` | **8** messages | Most-recent messages always preserved |

The budget itself is dynamic: `effective_ctx × chars_per_token`, floored at
`TOTAL_PAYLOAD_LIMIT_CHARS`. The `chars_per_token` factor is a per-model value
calibrated from real `usage.prompt_tokens` counts (see
[`token_calibration.rs`](src/token_calibration.rs)), falling back to the static
default of 4 until a model has been observed.

**Algorithm:**

1. **Budget gate** — while the whole payload fits within budget it is
   forwarded **unchanged**; no history is elided while there is room.
2. **Oldest-first trim** — only when over budget, unprotected `role: "tool"` /
   `role: "assistant"` messages whose `content` string exceeds 2,000
   characters are replaced **from oldest to newest**, stopping as soon as the
   payload drops back under budget, with:

   > `[Raw tool output truncated by proxy to maintain context window. Rely on your previous observations.]`

3. `role: "system"` messages and the last 8 messages are never modified.
4. Array-form content (multi-part messages) and `tool_calls` fields are
   never touched.
5. If the payload still exceeds the budget after every eligible message is
   trimmed, the request is **rejected** with HTTP 400:
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

## Proxy Dashboard

```text
GET /v1/proxy/status         (JSON snapshot)
GET /v1/proxy/status/stream  (SSE, hydrate-then-stream)
```

Both endpoints return the same [`DashboardSnapshot`](src/dashboard.rs) shape
— the single, unified data contract consumed by both `gglib proxy dashboard`
(CLI, `crates/gglib-cli/src/handlers/proxy_dashboard.rs`) and the web GUI's
Proxy Dashboard modal (`src/components/ProxyDashboardModal.tsx`). It fully
replaces the old `{snapshots, total_requests}` shape — there is no
backwards-compatible shim, since nothing consumed the old shape (it was
explicitly documented as a not-yet-consumed "future" contract).

- `GET /v1/proxy/status` returns one snapshot, computed on demand.
- `GET /v1/proxy/status/stream` is a Server-Sent Events stream: the first
  event is always the current snapshot (hydration), followed by a fresh
  snapshot roughly once per second for as long as the client stays
  connected (via [`gglib_sse::Broadcaster::subscribe_with_hydration`],
  see `crates/gglib-sse`). Keepalive comments (`: ping`) are sent every 30s
  on idle connections; native `EventSource` clients (browsers, and this is
  what the web GUI uses) ignore these transparently.

### Response shape

```json
{
  "active_connections": [
    {
      "id": "5b1b6f0e-9b1a-4e9a-8f7b-9c9f9a9b9c9d",
      "model_name": "qwen-3b",
      "started_at_secs": 1749283200,
      "is_streaming": true,
      "num_ctx": 8192,
      "phase": "processing_prompt",
      "prompt_processed": 512,
      "prompt_total": 2048,
      "prompt_cached": 0,
      "prompt_time_ms": 340
    }
  ],
  "slots_available": true,
  "slots": [
    {
      "id": 0,
      "id_task": 7,
      "n_ctx": 8192,
      "is_processing": true,
      "n_past": null,
      "cache_tokens": null,
      "next_token": { "n_decoded": 512 }
    }
  ],
  "slots_status": null,
  "recent_requests": [
    {
      "model_name": "qwen-3b",
      "payload_chars_before": 52000,
      "payload_chars_after": 8400,
      "messages_truncated": 3,
      "was_clamped": false,
      "recorded_at_secs": 1749283200
    }
  ],
  "total_requests": 42,
  "cache": {
    "disk_enabled": true,
    "disk_suppressed_for_model": true,
    "ram_budget_mb": 70008,
    "ram_state": "healthy",
    "needs_attention": true,
    "warnings": [
      "Disk cache offloading is disabled for this model — its attention keeps only part of the token history, which llama-server's slot files can't restore."
    ]
  }
}
```

#### `DashboardSnapshot`

| Field | Type | Description |
|-------|------|-------------|
| `active_connections` | array | Every currently in-flight `/v1/chat/completions` request (direct and council/virtual-model) |
| `slots_available` | `bool` | `true` if the running llama-server's `/slots` endpoint is reachable and enabled |
| `slots` | array | Per-slot context usage; empty unless `slots_available` is `true` |
| `slots_status` | `string \| null` | Reason `slots` is empty (disabled via `--no-slots`, or the poller's last connect/timeout/parse error); `null` when `slots_available` |
| `recent_requests` | array | Last ≤ 20 requests processed by the truncation pipeline, oldest-first |
| `total_requests` | `u64` | All requests since proxy start, including evicted ones |
| `cache` | `object \| null` | Prompt-cache configuration for the running model; `null` until the first request resolves one |

#### `cache` (`CacheStatus`)

Configuration state only — resolved when a model launches, changing only on a
model swap. Per-request cache telemetry (tokens reused, TTFT saved) will
extend this object rather than sitting beside it.

Replaces the former top-level `cache_enabled` boolean; `cache.disk_enabled`
carries the same information.

| Field | Type | Description |
|-------|------|-------------|
| `disk_enabled` | `bool` | Whether disk KV slot persistence is enabled on this proxy instance (`--cache` + `--slot-dir`) |
| `disk_suppressed_for_model` | `bool` | Disk layer enabled proxy-wide but skipped for this model (sliding-window/hybrid/recurrent attention). Always `false` when `disk_enabled` is `false` |
| `ram_budget_mb` | `u64 \| null` | Resolved `--cache-ram` budget; `null` when no flag was emitted and llama-server's own default applies |
| `ram_state` | `"healthy" \| "low" \| "disabled_insufficient_ram" \| "disabled_by_user" \| "llama_default"` | Budget health, for styling |
| `needs_attention` | `bool` | Whether anything here warrants surfacing. `false` for healthy budgets *and* for a cache the user deliberately disabled |
| `warnings` | `string[]` | Ready-to-render lines; empty when nothing is wrong. A low budget on a suppressed-disk model yields both warnings, since fixing one still leaves the other |

#### `active_connections[]` (`ActiveConnectionSnapshot`)

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` (UUID) | Assigned at registration |
| `model_name` | `string` | Model (or virtual council model) serving this connection |
| `started_at_secs` | `u64` | Unix timestamp when registered |
| `is_streaming` | `bool` | `true` for a streaming (SSE) request |
| `num_ctx` | `u64 \| null` | Effective context size, when known (`null` for council virtual-model runs) |
| `phase` | `"queued" \| "processing_prompt" \| "generating"` | Lifecycle phase |
| `prompt_processed` | `u32 \| null` | Tokens processed so far (from the most recent prompt-progress frame) |
| `prompt_total` | `u32 \| null` | Total prompt tokens |
| `prompt_cached` | `u32 \| null` | Tokens served from the KV cache |
| `prompt_time_ms` | `u64 \| null` | Wall-clock milliseconds elapsed |

#### `slots[]` (`SlotSnapshot`, mirrors llama.cpp's `GET /slots`)

| Field | Type | Description |
|-------|------|-------------|
| `id` | `i64` | Slot index within the running llama-server |
| `id_task` | `i64 \| null` | ID of the task currently occupying this slot |
| `n_ctx` | `u64 \| null` | Context size configured for this slot |
| `is_processing` | `bool` | Whether this slot is actively processing a request |
| `n_past` | `u64 \| null` | Legacy tokens-in-KV-cache field (older llama.cpp versions) |
| `cache_tokens` | `u64 \| null` | Alternate legacy field name for the same thing |
| `next_token.n_decoded` | `u64 \| null` | Current-schema generation-progress field |

`/slots`'s JSON shape varies across llama.cpp versions, so consumers should
compute "tokens in use" via the same priority-fallback chain the proxy itself
uses (`SlotSnapshot::tokens_in_use()`): `n_past` → `cache_tokens` →
`next_token.n_decoded`, `None` if none are present. Context remaining is
`n_ctx.saturating_sub(tokens_in_use())` when both are known
(`SlotSnapshot::context_remaining()`).

#### `recent_requests[]` (`ContextSnapshot`)

| Field | Type | Description |
|-------|------|-------------|
| `model_name` | `string` | Model targeted by the request |
| `payload_chars_before` | `usize` | Body size before truncation |
| `payload_chars_after` | `usize` | Body size after truncation |
| `messages_truncated` | `usize` | Number of messages whose content was replaced |
| `was_clamped` | `bool` | `true` when HTTP 400 was returned to the client |
| `recorded_at_secs` | `u64` | Unix timestamp of the observation |

The underlying ring buffer retains at most 50 entries; `recent_requests`
surfaces the newest 20 of those. `total_requests` grows monotonically
regardless of evictions.

### Consuming the stream

```bash
curl -N http://localhost:8080/v1/proxy/status/stream
```

- **CLI**: `gglib proxy dashboard [--host HOST] [--port PORT]` — see
  [`gglib-cli`](../gglib-cli/README.md#proxy-dashboard).
- **Web GUI**: click "View Dashboard" in the Proxy control's dropdown
  (`src/components/ProxyControl.tsx`), which opens `ProxyDashboardModal` —
  a native browser `EventSource` connected directly to the proxy's own
  port, independent of the app's own backend/Tauri IPC.
