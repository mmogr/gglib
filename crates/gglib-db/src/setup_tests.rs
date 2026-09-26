//! Tests for [`super`] — the schema and the migrations that bring an older
//! database up to it.
//!
//! Beside the module rather than inside it, the way `metrics_tests.rs` sits
//! beside `metrics.rs` in gglib-proxy. Declared as `mod tests` through
//! `#[path]`, so every test keeps the `setup::tests::` path it had when it
//! lived inline. The model-key backfill's tests are in
//! `setup_backfill_tests.rs`: the two subjects together are 399 lines, over
//! the 300 a new file may be.

use super::*;

#[tokio::test]
async fn test_setup_test_database() {
    let pool = setup_test_database().await.unwrap();

    // Verify tables exist by querying them
    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM models")
        .fetch_one(&pool)
        .await
        .unwrap();

    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings_kv")
        .fetch_one(&pool)
        .await
        .unwrap();

    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mcp_servers")
        .fetch_one(&pool)
        .await
        .unwrap();
}

/// How many times `column` appears in `table` — 1 proves the migration
/// landed exactly once, 0 that it never ran.
async fn column_count(pool: &SqlitePool, table: &str, column: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
        .bind(table)
        .bind(column)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A column the table does not have is added, and the value a
/// pre-migration row reads back is NULL rather than an error.
#[tokio::test]
async fn add_column_if_missing_adds_an_absent_column() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t (id) VALUES (1)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(column_count(&pool, "t", "note").await, 0);

    add_column_if_missing(&pool, "t", "note", "TEXT")
        .await
        .unwrap();

    assert_eq!(column_count(&pool, "t", "note").await, 1);
    let note: Option<String> = sqlx::query_scalar("SELECT note FROM t WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(note, None);
}

/// A column that is already there is a skip, not an error. This is the
/// idempotence the swallowed ALTER used to buy by discarding `duplicate
/// column name` — bought here by introspection instead, so it costs no
/// other error.
#[tokio::test]
async fn add_column_if_missing_skips_a_column_that_is_present() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, note TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t (id, note) VALUES (1, 'kept')")
        .execute(&pool)
        .await
        .unwrap();

    add_column_if_missing(&pool, "t", "note", "TEXT")
        .await
        .unwrap();
    add_column_if_missing(&pool, "t", "note", "TEXT")
        .await
        .unwrap();

    assert_eq!(column_count(&pool, "t", "note").await, 1);
    let note: String = sqlx::query_scalar("SELECT note FROM t WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(note, "kept", "the existing column must not be rewritten");
}

/// The #796 case. The `applied_json` ALTER once ran before the CREATE that
/// makes `benchmark_runs`; `no such table` went into `_` and every fresh
/// install shipped without the column. It must fail the boot it is made
/// on, not skip.
#[tokio::test]
async fn add_column_if_missing_fails_when_the_table_does_not_exist() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    let err = add_column_if_missing(&pool, "benchmark_runs", "applied_json", "TEXT")
        .await
        .expect_err("an ALTER against a table that does not exist must surface");

    assert!(
        err.to_string().contains("no such table"),
        "the real cause must reach the caller, got: {err}"
    );
}

/// A chat schema too old to write to must stop the boot, not be deleted.
///
/// The guard this replaces `DROPped` both chat tables — every conversation
/// and every message the user had — on a substring match against a stored
/// CREATE statement, with no prompt and no log line. The refusal names the
/// file and leaves the data where it is.
#[tokio::test]
async fn an_out_of_date_chat_schema_is_refused_rather_than_dropped() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // The pre-#362 shape: no 'tool' in the role CHECK.
    sqlx::query(
        "CREATE TABLE chat_conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id INTEGER NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant')),
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO chat_conversations (title) VALUES ('Yesterday')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages (conversation_id, role, content) \
             VALUES (1, 'user', 'do not delete me')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = create_schema(&pool)
        .await
        .expect_err("a chat schema this build cannot write to must stop the boot");
    assert!(
        err.to_string().contains("chat_messages"),
        "the message must name what is wrong, got: {err}"
    );

    let messages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    let conversations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(messages, 1, "the history must survive the refusal");
    assert_eq!(conversations, 1, "the history must survive the refusal");
}

/// The current shape is not refused — the check has to stay off the path
/// every real database takes, or it is just an outage.
#[tokio::test]
async fn a_current_chat_schema_is_left_alone() {
    let pool = setup_test_database().await.unwrap();
    sqlx::query("INSERT INTO chat_conversations (title) VALUES ('Today')")
        .execute(&pool)
        .await
        .unwrap();

    create_schema(&pool).await.unwrap();

    let conversations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(conversations, 1);
}

/// The `template_caps` migration on a database created **before** the
/// column existed: the post-hoc ALTER adds it, and rows written under the
/// old schema keep reading — their NULL is the tri-state's "never
/// observed", not an error.
#[tokio::test]
async fn template_caps_migration_upgrades_a_pre_caps_database() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // An old build's models table: the current schema minus
    // `template_caps`, one row already in it. Stamped at the canonical
    // schema version so the path backfills stay out of this test's way.
    sqlx::query(
        "CREATE TABLE models (
                id INTEGER PRIMARY KEY, name TEXT NOT NULL, file_path TEXT NOT NULL,
                param_count_b REAL NOT NULL, architecture TEXT, quantization TEXT,
                context_length INTEGER, inference_defaults TEXT, defaults_origin TEXT,
                server_defaults TEXT, expert_count INTEGER, expert_used_count INTEGER,
                expert_shared_count INTEGER, metadata TEXT, added_at TEXT NOT NULL,
                hf_repo_id TEXT, hf_commit_sha TEXT, hf_filename TEXT, download_date TEXT,
                last_update_check TEXT, tags TEXT DEFAULT '[]', model_key TEXT NOT NULL,
                file_paths_json TEXT, capabilities INTEGER DEFAULT 0, dialect_spec TEXT
            )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "PRAGMA user_version = {CANONICAL_PATH_SCHEMA_VERSION}"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
             VALUES ('Old', '/m/old.gguf', 7.0, ?, 'k')",
    )
    .bind(chrono::Utc::now().to_string())
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(column_count(&pool, "models", "template_caps").await, 0);

    create_schema(&pool).await.unwrap();

    assert_eq!(column_count(&pool, "models", "template_caps").await, 1);
    let caps: Option<String> =
        sqlx::query_scalar("SELECT template_caps FROM models WHERE name = 'Old'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(caps, None, "a pre-migration row reads as never observed");
}

/// Running the schema twice over one database must neither fail nor
/// duplicate the column — the ignore-if-exists ALTER bargain, exercised.
#[tokio::test]
async fn template_caps_migration_is_idempotent_on_an_existing_database() {
    let pool = setup_test_database().await.unwrap();
    assert_eq!(column_count(&pool, "models", "template_caps").await, 1);

    create_schema(&pool).await.unwrap();

    assert_eq!(column_count(&pool, "models", "template_caps").await, 1);
}
