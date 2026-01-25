# gglib-db

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json)

`SQLite` repository implementations for gglib domain types.

## Architecture

This crate is in the **Infrastructure Layer** — it implements the repository ports defined in `gglib-core`.

```text
gglib-core (ports)          gglib-db (adapters)           Adapters
┌──────────────────┐        ┌──────────────────┐        ┌──────────────────┐
│ ModelRepository  │◄───────│ SqliteModelRepo  │◄───────│    gglib-cli     │
│ McpServerRepo    │        │ SqliteMcpRepo    │        │   gglib-axum     │
│ ConversationRepo │        │ SqliteConvRepo   │        │   gglib-tauri    │
└──────────────────┘        └──────────────────┘        └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-db                                            │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                           repositories/                                     │    │
│  │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐  ┌─────────────┐   │    │
│  │  │  model_repo   │  │   mcp_repo    │  │  conv_repo    │  │ settings_   │   │    │
│  │  │  SqliteModel  │  │  SqliteMcp    │  │  SqliteConv   │  │   repo      │   │    │
│  │  │   Repository  │  │  Repository   │  │  Repository   │  │             │   │    │
│  │  └───────────────┘  └───────────────┘  └───────────────┘  └─────────────┘   │    │
│  └─────────────────────────────────────────────────────────────────────────────┘    │
│                                                                                     │
│  ┌───────────────┐  ┌───────────────┐                                               │
│  │   factory.rs  │  │   setup.rs    │                                               │
│  │  Connection   │  │   Migrations  │                                               │
│  │   pooling     │  │   & schema    │                                               │
│  └───────────────┘  └───────────────┘                                               │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                depends on
                                          ▼
                              ┌───────────────────┐
                              │    gglib-core     │
                              │  (port traits)    │
                              └───────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`factory.rs`](src/factory) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-coverage.json) |
| [`setup.rs`](src/setup) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-coverage.json) |
| [`repositories/`](src/repositories/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`factory.rs`** — Database connection factory and pooling
- **`setup.rs`** — Schema migrations and database initialization
- **`repositories/`** — `SQLite` implementations of all repository ports

## Features

- **Async `SQLite`** — Uses `sqlx` with async/await for non-blocking database access
- **Trait Implementations** — Each repository implements its `gglib-core` port trait
- **Connection Pooling** — Factory provides pooled connections for concurrent access
- **Auto-Migration** — Schema setup runs automatically on first connection

## Usage

```rust,no_run
use gglib_db::setup_database;
use gglib_db::repositories::SqliteModelRepository;
use gglib_core::ports::ModelRepository;
use std::path::Path;

async fn example() {
    // Initialize database
    let pool = setup_database(Path::new("gglib.db")).await.unwrap();

    // Use repository via trait
    let repo = SqliteModelRepository::new(pool);
    let models = repo.list().await.unwrap();
}
```

## Design Decisions

1. **Port Pattern** — Repositories implement traits from `gglib-core`, not local traits
2. **No Domain Logic** — Pure data access; business logic stays in `gglib-core::services`
3. **Pooled Connections** — All adapters share a connection pool for efficiency
