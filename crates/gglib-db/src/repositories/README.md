# repositories

<!-- module-docs:start -->

Repository implementations using SQLite.

These implementations encapsulate all SQL queries and database access. The `SqlitePool` is confined to this module and never exposed through port trait signatures.

## Implementations

| Repository | Port |
|------------|------|
| `SqliteModelRepository` | `ModelRepository` |
| `SqliteMcpRepository` | `McpRepository` |
| `SqliteChatHistoryRepository` | `ChatHistoryRepository` |
| `SqliteSettingsRepository` | `SettingsRepository` |
| `SqliteDownloadStateRepository` | `DownloadStateRepositoryPort` |

## Submodules

| Module | Description |
|--------|-------------|
| `row_mappers` | SQL row to domain type conversions |

## Design

- SQL is fully encapsulated — domain types in/out only
- Async via `sqlx` with connection pooling
- Migrations managed separately

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`row_mappers.rs`](row_mappers) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-row_mappers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-row_mappers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-row_mappers-coverage.json) |
| [`sqlite_chat_history_repository.rs`](sqlite_chat_history_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_chat_history_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_chat_history_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_chat_history_repository-coverage.json) |
| [`sqlite_download_state_repository.rs`](sqlite_download_state_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_download_state_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_download_state_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_download_state_repository-coverage.json) |
| [`sqlite_mcp_repository.rs`](sqlite_mcp_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_mcp_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_mcp_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_mcp_repository-coverage.json) |
| [`sqlite_model_repository.rs`](sqlite_model_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_model_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_model_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_model_repository-coverage.json) |
| [`sqlite_settings_repository.rs`](sqlite_settings_repository) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_settings_repository-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_settings_repository-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-repositories-sqlite_settings_repository-coverage.json) |
<!-- module-table:end -->

</details>
