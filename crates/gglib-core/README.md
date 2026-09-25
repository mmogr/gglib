# gglib-core

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json)

Pure domain types, ports, and traits for gglib — the foundation of the hexagonal architecture.

The product's judgment lives here as pure logic: sampling resolution, dialect
normalization, GGUF capability detection, and residency policy — all decidable
without a GPU, a network, or a running llama-server, and therefore testable.

## Architecture

This crate is the **Core Layer** — the innermost ring of the architecture. It depends on no other gglib crate, and every other workspace crate except `gglib-build-info`, `gglib-sse` and `gglib-tauri` depends on it (`gglib-integration-tests` as a dev-dependency).

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    Core Layer                                       │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                         ►►► gglib-core ◄◄◄                                  │   │
│   │        Domain types, ports & traits (no database, HTTP or UI crates)        │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          ▲
                                          │
              ┌───────────────────────────────────────────────────────┐
              │  gglib-db, gglib-gguf, gglib-hf, gglib-mcp,           │
              │  gglib-download, gglib-runtime                        │
              └───────────────────────────────────────────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

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
- **`services/`** — Application use cases and business logic orchestration (model management, server lifecycle, chat history, settings, model verification & repair)
- **`events/`** — Strongly-typed application events for UI/adapter notification
- **`paths/`** — Path configuration and platform-specific directory handling
- **`download/`** — Download-related DTOs and progress tracking types
- **`ports/mcp_dto.rs`** — Cross-boundary DTOs for MCP resolution status (Tauri/Axum/TypeScript)
- **`sse/`** — OpenAI-compatible SSE codec: byte-stream `SseStreamDecoder`, single-frame `parse_sse_frame`, and `SseEncoder` that re-emits canonical `chat.completion.chunk` envelopes. Used by the proxy's universal consistency layer.
- **`normalize/`** — Universal normalization layer. The `ToolCallParser` trait plus dialect parsers (`StandardJsonParser` identity and the spec-driven `DelimitedToolCallParser` for marker-delimited tool calls) rewrite model-specific output into strict `OpenAI` events. Selected per-request from the model's persisted `DialectSpec` (with a `format:*` tag fallback) via `normalize::registry::get_parser`.
- **`utils/`** — Shared utility functions and helpers
- **`settings.rs`** — Application settings and configuration types

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
