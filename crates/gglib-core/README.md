# gglib-core

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json)

Pure domain types, ports, and traits for gglib — the foundation of the hexagonal architecture.

## Architecture

This crate is the **Core Layer** — the innermost ring of the architecture. All other crates depend on it; it depends on none.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    Core Layer                                       │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                         ►►► gglib-core ◄◄◄                                  │   │
│   │              Pure domain types, ports & traits (no infra deps)              │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
              ┌───────────────────────────────────────────────────────┐
              │  gglib-db, gglib-gguf, gglib-hf, gglib-mcp,           │
              │  gglib-download, gglib-runtime                        │
              └───────────────────────────────────────────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

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

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`settings.rs`](src/settings) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-settings-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-settings-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-settings-coverage.json) |
| [`contracts/`](src/contracts/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-contracts-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-contracts-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-contracts-coverage.json) |
| [`domain/`](src/domain/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-coverage.json) |
| [`download/`](src/download/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-download-coverage.json) |
| [`events/`](src/events/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-events-coverage.json) |
| [`paths/`](src/paths/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-paths-coverage.json) |
| [`ports/`](src/ports/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-ports-coverage.json) |
| [`services/`](src/services/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-services-coverage.json) |
| [`utils/`](src/utils/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-utils-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`domain/`** — Pure domain types: `Model`, `ModelFile`, `McpServer`, `Conversation`
- **`ports/`** — Trait definitions (repository ports, HF client port, event emitter)
- **`services/`** — Application use cases and business logic orchestration
- **`events/`** — Strongly-typed application events for UI/adapter notification
- **`paths/`** — Path configuration and platform-specific directory handling
- **`download/`** — Download-related DTOs and progress tracking types
- **`ports/mcp_dto.rs`** — Cross-boundary DTOs for MCP resolution status (Tauri/Axum/TypeScript)
- **`utils/`** — Shared utility functions and helpers
- **`settings.rs`** — Application settings and configuration types

## Design Principles

1. **No Infrastructure Dependencies** — No `SQLx`, no HTTP clients, no filesystem I/O
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
