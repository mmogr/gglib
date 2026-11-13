# gglib-db

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json)

Layer: **infrastructure** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

`SQLite` repository implementations for gglib domain types.

One database behind every surface, which is why a model tagged in the GUI is
tagged for the proxy too, and why per-model inference defaults survive a
restart.

## Architecture

This crate is in the **Infrastructure Layer** — it implements the repository ports defined in `gglib-core`.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-db                                            │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────────────────────────────────────────────────────────────────────┐    │
│  │                           repositories/                                     │    │
│  │                                                                             │    │
│  │   sqlite_model_repository.rs          SqliteModelRepository                 │    │
│  │   sqlite_mcp_repository.rs            SqliteMcpRepository                   │    │
│  │   sqlite_chat_history_repository.rs   SqliteChatHistoryRepository           │    │
│  │   sqlite_settings_repository.rs       SqliteSettingsRepository              │    │
│  │   sqlite_benchmark_repository.rs      SqliteBenchmarkRepository             │    │
│  │   sqlite_attachment_store.rs          SqliteAttachmentStore                 │    │
│  │   sqlite_loop_guard_trip_log.rs       SqliteLoopGuardTripLog                │    │
│  │   model_files_repository.rs           ModelFilesRepository                  │    │
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

**Module Descriptions:**
- **`daemon_startup.rs`** — What the daemon, and only the daemon, puts right when it starts: a benchmark run a crash left `running`, and stored images no message carries
- **`database_file.rs`** — The database and its directory made private to this user before `SQLite` opens them
- **`factory.rs`** — Database connection factory and pooling
- **`loop_guard_trip_writer.rs`** — The loop guard's batched writer: the sink the proxy records into, and the task that writes and prunes the log
- **`setup.rs`** — Schema migrations and database initialization
- **`setup_backup.rs`** — The copy of the database kept beside it, `<name>.gen<N>.bak`, from before an upgrade raised its generation; the first copy of a generation is kept and never written over, other generations' are removed, and an older build's refusal names it and how to put it back
- **`setup_generation.rs`** — The schema generation, one number in its own `schema_generation` table that says which build's schema a file carries; a build that finds a higher one than its own refuses the file before touching it
- **`sqlite_init.rs`** — `SQLite` initialised once, on one thread, before `setup_database` or `setup_test_database` opens a pool
- **`setup_attachments.rs`** — The `attachments` table (an image's bytes under the SHA-256 of them) and `message_attachments` (the images each message carries, in order), and the sweep of images no message carries
- **`setup_models.rs`** — The `models` table: its definition, the columns added to it since (`image_family` the latest), its indexes, and the one-time rebuild that stops it reusing ids
- **`setup_model_files.rs`** — The `model_files` table, and `models.projector_path` with the one-time link of each model to the projector among its own files
- **`setup_model_components.rs`** — The `model_components` table: the files an image model draws with beside its weights, one row per role, gone with their model, indexed by path
- **`setup_branches.rs`** — The columns a branched chat is recorded by (ADR 0017), added after the chat tables
- **`setup_migrations.rs`** — The one-time passes over rows, numbered on `PRAGMA user_version`, and the runner that takes each once, in order, with its stamp
- **`setup_recipes.rs`** — The one-time clearing of stored copies of the `reasoning` recipe from `models` rows
- **`setup_settings.rs`** — Step 2: the one-time fold of the loop guard's `proxy_loop_detection` row into `loop_guard_mode`
- **`setup_loop_guard.rs`** — The loop guard's log tables, and step 7: the guard is on or off, with no mode left in settings, the log or stored reports
- **`setup_remote_grant.rs`** — The one-time copy of the remote tunnel's old list of MCP servers onto each server's own `allow_remote` flag
- **`setup_unknown_params.rs`** — The one-time change of parameter counts stored as `0.0` to unknown
- **`setup_format_tags.rs`** — Step 8: every `format:*` tag stripped from `models.tags`, the rest kept in order; text that is not a JSON array is left alone
- **`setup_mcp_servers.rs`** — The `mcp_servers` and `mcp_server_env` tables, and step 6: the stored SSE servers, which gglib never ran, deleted with their variables, `manual` servers made disabled `lazy` ones, and both tables rebuilt without `type` and `url`, keeping ids
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
use gglib_core::ports::{ModelRepository, NoopGgufParser};
use std::path::Path;

async fn example() {
    // Initialize database. The parser reads a model file's header where a
    // migration asks what the file is; the daemon passes the real one.
    let pool = setup_database(Path::new("gglib.db"), &NoopGgufParser)
        .await
        .unwrap();

    // Use repository via trait
    let repo = SqliteModelRepository::new(pool);
    let models = repo.list().await.unwrap();
}
```

## Design Decisions

1. **Port Pattern** — Repositories implement traits from `gglib-core`, not local traits
2. **No Domain Logic** — Pure data access; business logic stays in `gglib-core::services` — except the loop guard log's writer, which owns its batching and pruning; the retention length comes from `gglib-core`
3. **Pooled Connections** — All adapters share a connection pool for efficiency

## Schema Migrations

`create_schema()` is `CREATE TABLE IF NOT EXISTS` for the current shape, plus
one `add_column_if_missing()` call per column that was added after the fact,
and it is safe to run against a database of any older vintage. The one-time
passes over rows are one numbered list in `setup_migrations.rs` (below). A database a newer build stamped with a higher schema
generation is refused (`setup_generation.rs`).

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
`scripts/check_swallowed_db_errors.sh` fails the build if the discarded form
comes back.

**Schema setup deletes no user data.** `create_schema()` drops a table only when
that table is the tombstone of a removed feature and provably never held a row
(`download_queue`, the orchestrator pair). A schema this build cannot correctly
write to is refused instead: if `chat_messages` predates the `'tool'` role, setup
fails and names the database file, leaving every conversation where it is. That
branch used to DROP both chat tables — silently, at boot, on a substring match
against a stored CREATE statement. Beyond the two settings rows setup reclaims
(below), this crate deletes rows on its own in two places. The
loop guard's log is pruned by its writer, by age (90 days, today included) and
by a row cap, whole days at a time. A stored image no message carries, last
stored more than a day ago, is deleted when the daemon starts, and only then.
An image is stored before the message that names it is saved, and the CLI
stores one in its own process and may then start the daemon, so an unlinked
image must outlive both an open of the database and a daemon's start; storing
the same image again starts its day again.

**A model id is never reused.** `models.id` is `INTEGER PRIMARY KEY
AUTOINCREMENT`, so removing a model never frees its id for the next one: an id
kept in a conversation, a benchmark run, the default-model setting or on
another machine names that model or none. Registering a model that is already
there binds the row's own id, so an update takes no id from the sequence
either.

`SQLite` cannot add `AUTOINCREMENT` to a table in place, so a library whose
`models` table was made without it is rebuilt once, near the end of
`create_schema()`, after every table the rebuild reads exists:

- Foreign keys are off for the rebuild, set on one connection outside its
  transaction, so dropping the old table cascades into nothing and nulls no
  conversation's model. That connection is detached on any error, so none goes
  back to the pool with foreign keys off.
- `BEGIN IMMEDIATE`, then the shape is asked again, so of two processes opening
  one old library at once the second finds the work done.
- Every column the two tables share is copied by name, ids and
  `file_paths_json` included, and the indexes are made again.
- `PRAGMA foreign_key_check`, counted once the lock is held, must report no
  more rows than before. A row that already referenced no model does not stop
  a library booting; one the rebuild orphaned would.
- The sequence starts above the highest id anything holds: the models
  themselves, the `model_id` of every table that references them (a row that
  already referenced no model included, which a new model would otherwise take
  over), every benchmark run's `model_ids` and the `default_model_id` setting.

**A setting that is gone leaves no row, and takes no answer with it.**
`Settings` reads the rows it has fields for and passes over the rest, and a
write touches one row per field, so nothing else would ever remove a row whose
setting is gone. Two steps do:

- Step 2 folds `proxy_loop_detection`, the loop guard's first switch, into
  `loop_guard_mode`: a `false` switch with no mode is stored as `"off"`, and
  the switch's row is deleted.
- Step 7 makes the guard on or off: a `loop_guard_mode` of `"off"` becomes
  `loop_guard = false`, any other mode becomes no row (on), and the mode's row
  goes with every row of a setting deleted before it that nothing reads. It
  also drops the `mode` column from the loop guard's log, adding together scan
  counts that differed only by mode, and removes `loop_guard_mode` from stored
  agentic reports.

The `PRAGMA user_version` ladder is the list in `setup_migrations.rs`: a file
stamped `N` has had steps `1..=N`, each run once per database, in its own
`BEGIN IMMEDIATE` transaction with its stamp. `1` is the canonical-path
backfill (a blocking syscall per row, paid once per library), `2` the loop
guard's fold above, `3` the stored copies of the `reasoning` recipe cleared,
`4` the remote tunnel's old `--allow-mcp` list copied onto
`mcp_servers.allow_remote` (only from a machine whose tunnel was on, and only a
name exactly one server has), `5` the parameter counts stored as `0.0` made
unknown, `6` the stored SSE MCP servers and their variables deleted,
`manual` servers made disabled `lazy` ones, and `mcp_servers` rebuilt without
its `type` and `url` columns, `7` the loop guard made on or off (above), and
`8` every `format:*` tag stripped from `models.tags`.
Step 1 runs before the projector link,
which reads the paths it rewrites; the rest run last, after the `models`
rebuild. The column set is deliberately not on it. The `template_caps` column post-dates
the first stamp, so a database stamped `1` or later may or may not carry it,
and a version-gated `ALTER` that propagated errors would abort startup with
`duplicate column name` on real installs. A column is added by asking the
table its shape (`PRAGMA table_info`) instead.

## Testing

All tests are inline `#[cfg(test)]` blocks or `_tests.rs` siblings living alongside their
respective implementations, except three under `tests/` that need a real database on disk:
`tests/file_modes.rs`, which checks its modes through the public `setup_database()` as a caller
opens it; `tests/settings_writes.rs`, which runs two settings writers against one file
through pools of their own, as the daemon and the CLI are; and `tests/loop_guard_writer_race.rs`,
which races scans against the loop guard writer's stop in a database file of its own per round.

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
| `SqliteModelRepository` | insert/list, get_by_id, get_by_name, update, delete, not-found errors, upsert dedup, an upsert takes no id |
| `SqliteChatHistoryRepository` | create/list conversations, get by id, count, update title, delete, messages round-trip, update/delete messages |
| `SqliteMcpRepository` | insert/get/list/update/delete servers, a failed insert or update leaving nothing partial, a row written by SQL read back, a database holding two servers of one name |
| `SqliteSettingsRepository` | load empty, save and load, clear individual fields |
| `SqliteBenchmarkRepository` | `applied_json` through both read paths, an agentic run's type and report round-trip |
| `SqliteAttachmentStore` | an image's facts and bytes read back, one id stored twice is one row, storing again moves its last-stored time, an unknown id, a store failure, ids found by their start |
| `ModelFilesRepository` | insert and get, verification time, a file inserted again (changed or unchanged), database and decode failures as storage errors |
| `SqliteLoopGuardTripLog` | summary over both tables — a scanned day with no trips, and a trip whose scan was lost — detectors and models apart, the window, a second flush adding to a day, pruning by age and by cap, no text in any column |

