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
