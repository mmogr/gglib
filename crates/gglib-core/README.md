# gglib-core

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json)

Layer: **core** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

Pure domain types, ports, and traits for gglib — the foundation of the hexagonal architecture.

The product's judgment lives here as pure logic: sampling resolution, dialect
normalization, GGUF capability detection, and residency policy — all decidable
without a GPU, a network, or a running llama-server, and therefore testable.

## Architecture

This crate is the **Core Layer** — the innermost ring of the architecture. It depends on no other gglib crate, and every other workspace crate except `gglib-build-info`, `gglib-broadcast` and `gglib-test-http` depends on it (`gglib-integration-tests` as a dev-dependency).

<!-- crate-graph:start -->
Each band is a layer of `crates/layers.toml`, from the top down. Under the bands, each crate is listed with the workspace crates it names in a normal or build dependency, as its manifest lists them; dev-dependencies are left out.

```text
ADAPTER ──────────────────────────────────────────────────────────────────────
  axum               cli                ffi                src-tauri
FACADE ───────────────────────────────────────────────────────────────────────
  app-services       bootstrap
INFRASTRUCTURE ───────────────────────────────────────────────────────────────
  db                 download           engine-llama       gguf
  hf                 mcp                proxy              runtime
APPLICATION ──────────────────────────────────────────────────────────────────
  agent
CORE ─────────────────────────────────────────────────────────────────────────
  core
UTILITY ──────────────────────────────────────────────────────────────────────
  broadcast          build-info         test-http
TESTS ────────────────────────────────────────────────────────────────────────
  integration-tests

DEPENDS ON ───────────────────────────────────────────────────────────────────
  axum               → app-services, bootstrap, db, gguf, mcp, proxy, runtime,
                       core, broadcast, build-info
  cli                → axum, app-services, bootstrap, db, download, mcp, proxy,
                       runtime, core, build-info
  ffi                → bootstrap, engine-llama, mcp, proxy, core
  src-tauri          → axum, app-services, proxy, core, build-info
  app-services       → download, mcp, proxy, runtime, agent, core
  bootstrap          → db, download, gguf, hf, core
  db                 → core
  download           → hf, core
  engine-llama       → gguf, core
  gguf               → core
  hf                 → core
  mcp                → core, build-info
  proxy              → mcp, core, broadcast, build-info
  runtime            → mcp, proxy, agent, core
  agent              → core
  core               → none
  broadcast          → none
  build-info         → none
  test-http          → none
  integration-tests  → none
```
<!-- crate-graph:end -->

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              gglib-core                                             │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   domain/   │  │   ports/    │  │  services/  │  │   events/   │                 │
│  │  Pure types │  │   Traits    │  │  Use cases  │  │  App events │                 │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                 │
│         │                │                │                │                        │
│         ▼                ▼                ▼                ▼                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                 │
│  │   paths/    │  │  download/  │  │   utils/    │  │  settings   │                 │
│  │ Path config │  │Download DTOs│  │   Helpers   │  │   Config    │                 │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘                 │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Module Descriptions:**
- **`domain/`** — Pure domain types: `Model`, `ModelFile`, `McpServer`, `Conversation`; agent loop primitives: `AgentConfig`, `AgentMessage`, `AgentEvent`, `ToolDefinition`, `ToolCall`, `ToolResult`; and server configuration: `ServerConfig` (per-model launch defaults with `context_length`, used in the 5-level fallback chain: runtime request → model `server_defaults` → global settings → fitted to hardware → hardcoded `DEFAULT_CONTEXT_SIZE`)
- **`ports/`** — Trait definitions (repository ports, HF client port, event emitter, `AgentLoopPort` / `ToolExecutorPort` / `AgentError` for the backend agentic loop)
- **`services/`** — Application use cases and business logic orchestration (model management, server lifecycle, chat history, image attachments, settings, model verification & repair, and `CatalogPortImpl`, the model catalog port over a `ModelRepository`)
- **`events/`** — Strongly-typed application events for UI/adapter notification
- **`paths/`** — Path configuration and platform-specific directory handling
- **`download/`** — Download-related DTOs and progress tracking types
- **`ports/mcp_dto.rs`** — Cross-boundary DTOs for MCP resolution status (Tauri/Axum/TypeScript)
- **`sse/`** — OpenAI-compatible SSE codec: the byte-level splitter `DataFrames` (complete lines, then each event's `data:` payload), byte-stream `SseStreamDecoder` built on the same lines, single-frame `parse_sse_frame`, and `SseEncoder` that re-emits canonical `chat.completion.chunk` envelopes. Used by the proxy's universal consistency layer, by the daemon's runs and its compare benchmark, and by every event stream the CLI reads.
- **`normalize/`** — Universal normalization layer. The `ToolCallParser` trait plus dialect parsers (`StandardJsonParser` identity and the spec-driven `DelimitedToolCallParser` for marker-delimited tool calls) rewrite model-specific output into strict `OpenAI` events. Selected per-request from the model's persisted `DialectSpec` (with a `format:*` tag fallback) via `normalize::registry::get_parser`.
- **`utils/`** — Shared utility functions and helpers
- **`settings.rs`** — Application settings and configuration types
- **`secret.rs`** — The secrets gglib stores (`SettingSecret`, `SecretRef`), what a held one is shown as (`SHOWN_AS`), and the blank rule (`held`). `services::SecretsService::reveal` is the one way a stored value comes back (ADR 0027)
- **`hf_token.rs`** — The `HuggingFace` token: `from_env`, the one read of `HF_TOKEN`, and `normalised`, the one rule for what a value counts as (trimmed; a blank one is no token), whether it came from the environment or a caller. `AppCore` holds the token the bootstrap wired, its caller's or the environment's, and hands it out

## Design Principles

1. **No database, HTTP or UI crates** — no `sqlx`, no HTTP client or server, no CLI or desktop framework. It does local file I/O, such as data directories, the `.env` overrides file and device keys
2. **Trait-Based Ports** — All external capabilities defined as traits for DI
3. **Pure Data Types** — Domain types are serializable, cloneable, and testable
4. **Event-Driven** — `AppEventEmitter` trait enables decoupled UI updates

## Usage

```rust,no_run
use gglib_core::domain::Model;
use gglib_core::ports::{ModelRepository, RepositoryError};
use gglib_core::services::ModelService;
use gglib_core::events::AppEvent;

// Ports define capabilities
async fn example<R: ModelRepository>(repo: &R) -> Result<Vec<Model>, RepositoryError> {
    let models = repo.list().await?;
    Ok(models)
}
```
