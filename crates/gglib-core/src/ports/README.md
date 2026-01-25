# ports

Port trait definitions for gglib's hexagonal architecture.

## Purpose

This module defines all the **port traits** (interfaces) that the core domain needs from external adapters. Following hexagonal architecture principles, the core domain depends on these abstractions, not on concrete implementations.

## Architecture Pattern

**Ports = Interfaces the core needs from the outside world**

```text
┌─────────────────────────────────────┐
│         gglib-core (Domain)         │
│  ┌───────────────────────────────┐  │
│  │   Business Logic/Services     │  │
│  └───────────┬───────────────────┘  │
│              │ depends on           │
│              ▼                       │
│  ┌───────────────────────────────┐  │
│  │      Port Traits (this)       │  │
│  └───────────────────────────────┘  │
└──────────────┬──────────────────────┘
               │ implemented by
               ▼
┌──────────────────────────────────────┐
│      Adapters (Infrastructure)       │
│  gglib-db, gglib-hf, gglib-runtime   │
└──────────────────────────────────────┘
```

## Port Categories

### Repository Ports (Data Persistence)
- **`model_repository.rs`** - Model CRUD operations
- **`mcp_repository.rs`** - MCP server configuration storage
- **`chat_history.rs`** - Conversation history persistence
- **`settings_repository.rs`** - Application settings storage

### External Service Ports
- **`huggingface/`** - HuggingFace API client interface
  - `client.rs` - Main client trait
  - `error.rs` - HF-specific error types
  - `types.rs` - HF domain types
- **`gguf_parser.rs`** - GGUF file parsing capability

### Runtime Ports (Process Management)
- **`model_runtime.rs`** - Start/stop model servers
- **`process_runner.rs`** - Generic process execution
- **`server_health.rs`** - Server health checking
- **`server_log_sink.rs`** - Log streaming interface

### Catalog & Discovery
- **`model_catalog.rs`** - Model discovery and metadata
- **`model_registrar.rs`** - Model registration interface

### Download Management
- **`download.rs`** - Download operations
- **`download_manager.rs`** - Download queue management
- **`download_state.rs`** - Download state tracking
- **`download_event_emitter.rs`** - Download progress events

### Event System
- **`event_emitter.rs`** - Generic app event emission
- **`tool_support.rs`** - Tool/capability discovery

### System Capabilities
- **`system_probe.rs`** - System information (GPU, OS, etc.)

### Error Types
- **`mcp_error.rs`** - MCP-specific error definitions

## Design Principles

1. **Dependency Inversion**: Core depends on abstractions, not implementations
2. **Testability**: All traits can be mocked for unit testing
3. **Async by Default**: All traits use `async_trait` for async operations
4. **Error Handling**: Each port defines its own error types
5. **Trait Objects**: Most ports use `dyn Trait` for runtime polymorphism

## Example Usage

```rust
use gglib_core::ports::{ModelRepository, RepositoryError};
use gglib_core::domain::Model;

async fn use_port<R: ModelRepository>(repo: &R) -> Result<Vec<Model>, RepositoryError> {
    repo.list().await
}
```

## Implementation

Concrete implementations are in:
- **gglib-db**: Repository ports (`SqlxModelRepository`, etc.)
- **gglib-hf**: HuggingFace client port
- **gglib-runtime**: Runtime/process ports
- **gglib-download**: Download management ports

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`chat_history.rs`](chat_history) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-chat_history-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-chat_history-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-chat_history-coverage.json) |
| [`download_event_emitter.rs`](download_event_emitter) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_event_emitter-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_event_emitter-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_event_emitter-coverage.json) |
| [`download_manager.rs`](download_manager) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_manager-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_manager-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_manager-coverage.json) |
| [`download.rs`](download) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download-coverage.json) |
| [`download_state.rs`](download_state) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_state-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_state-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-download_state-coverage.json) |
| [`event_emitter.rs`](event_emitter) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-event_emitter-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-event_emitter-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-event_emitter-coverage.json) |
| [`gguf_parser.rs`](gguf_parser) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-gguf_parser-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-gguf_parser-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-gguf_parser-coverage.json) |
| [`mcp_dto.rs`](mcp_dto) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_dto-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_dto-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_dto-coverage.json) |
| [`mcp_error.rs`](mcp_error) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_error-coverage.json) |
| [`mcp_repository.rs`](mcp_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-mcp_repository-coverage.json) |
| [`model_catalog.rs`](model_catalog) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_catalog-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_catalog-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_catalog-coverage.json) |
| [`model_registrar.rs`](model_registrar) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_registrar-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_registrar-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_registrar-coverage.json) |
| [`model_repository.rs`](model_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_repository-coverage.json) |
| [`model_runtime.rs`](model_runtime) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_runtime-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_runtime-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-model_runtime-coverage.json) |
| [`process_runner.rs`](process_runner) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-process_runner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-process_runner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-process_runner-coverage.json) |
| [`server_health.rs`](server_health) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_health-coverage.json) |
| [`server_log_sink.rs`](server_log_sink) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_log_sink-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_log_sink-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-server_log_sink-coverage.json) |
| [`settings_repository.rs`](settings_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-settings_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-settings_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-settings_repository-coverage.json) |
| [`system_probe.rs`](system_probe) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-system_probe-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-system_probe-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-system_probe-coverage.json) |
| [`tool_support.rs`](tool_support) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-tool_support-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-tool_support-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-tool_support-coverage.json) |
| [`huggingface/`](huggingface/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-huggingface-coverage.json) |
<!-- module-table:end -->
