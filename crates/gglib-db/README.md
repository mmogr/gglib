# gglib-db

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json)

`SQLite` repository implementations for gglib domain types.

One database behind every surface, which is why a model tagged in the GUI is
tagged for the proxy too, and why per-model inference defaults survive a
restart.

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

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

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
| [`factory.rs`](src/factory.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-factory-coverage.json) |
| [`setup.rs`](src/setup.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-setup-coverage.json) |
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

## Schema Migrations

There is no migration runner. `create_schema()` is `CREATE TABLE IF NOT EXISTS`
for the current shape, plus one `add_column_if_missing()` call per column that
was added after the fact, and it is safe to run against a database of any
vintage.

**An `ALTER` that fails is a failure.** `add_column_if_missing()` reads `PRAGMA
table_info`, returns without doing anything if the column is already there, and
otherwise runs the `ALTER` with `?` — so the idempotence comes from
introspection and every error still surfaces. Those six migrations used to be
written the other way round:

```text
let _ = sqlx::query("ALTER TABLE …").execute(pool).await;
// Ignore error if column already exists
```

which absorbed `no such table`, `database is locked` and `database or disk is
full` on exactly the same terms as the duplicate column it named. #796 is what
that cost: an `ALTER` placed above the `CREATE` that makes its table failed
silently, so every fresh install ran without `benchmark_runs.applied_json`
until a second boot re-ran the migration.

`is_unique_violation()` is the sanctioned shape of tolerance — one error code,
named, with every other one propagated.

There is deliberately no `PRAGMA user_version` ladder over the column set.
`CANONICAL_PATH_SCHEMA_VERSION` is already load-bearing for the canonical-path
backfill (a blocking syscall per row, paid once per library), and the
`template_caps` column post-dates the stamp — so a version-gated `ALTER` that
propagated errors would abort startup with `duplicate column name` on real
installs. A ladder can be layered on after v1 at no cost, because `PRAGMA
table_info` keeps the shape introspectable either way.

## Testing

All tests are inline `#[cfg(test)]` blocks living alongside their respective implementations.

### Test harness

Use `setup_test_database()` (feature-gated under `test-utils`) for the test harness.
`setup_test_database()` creates an in-memory `SQLite` database and runs the full production schema
via `create_schema()`, so every test exercises the real schema including all columns and CHECK
constraints.

```rust,no_run
#[cfg(test)]
mod tests {
    use crate::setup::setup_test_database;
    use super::*;

    #[tokio::test]
    async fn example() {
        let pool = setup_test_database().await.unwrap();
        let repo = SqliteModelRepository::new(pool);
        // ...
    }
}
```

### Coverage at a glance

| Repository | Tests |
|---|---|
| `SqliteModelRepository` | insert/list, get_by_id, get_by_name, update, delete, not-found errors, upsert dedup |
| `SqliteChatHistoryRepository` | create/list conversations, get by id, count, update title, delete, messages round-trip, update/delete messages |
| `SqliteMcpRepository` | insert/get/list/update/delete servers, SSE server, duplicate name conflict |
| `SqliteSettingsRepository` | load empty, save and load, clear individual fields |

