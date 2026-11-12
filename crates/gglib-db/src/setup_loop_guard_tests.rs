//! Step 7 on a library of the test's own, its log tables put back in the
//! shape a build with modes left them: what each part of the step leaves.
//! The 0.33.0 fixture's upgrade is `setup_fixture_*_tests.rs`.

use sqlx::SqlitePool;

use super::super::migrations::{LATEST, run};
use crate::setup::setup_test_database;

/// The log tables as 0.33.0 made them, with a `mode` column.
const OLD_TABLES: &str = "
DROP TABLE loop_guard_trips;
DROP TABLE loop_guard_scans;
CREATE TABLE loop_guard_trips (
    id INTEGER PRIMARY KEY, recorded_at INTEGER NOT NULL, model_name TEXT NOT NULL,
    gglib_version TEXT NOT NULL, mode TEXT NOT NULL, detector TEXT NOT NULL,
    signature_hash TEXT, session_hash TEXT, repeat_count INTEGER, threshold INTEGER
);
CREATE INDEX idx_loop_guard_trips_recorded_at ON loop_guard_trips(recorded_at);
CREATE TABLE loop_guard_scans (
    day INTEGER NOT NULL, model_name TEXT NOT NULL, gglib_version TEXT NOT NULL,
    mode TEXT NOT NULL, scanned INTEGER NOT NULL,
    PRIMARY KEY (day, model_name, gglib_version, mode)
);
";

/// A library one step short of this build's, with the old log tables.
async fn before_step_7(planted: &str) -> SqlitePool {
    let pool = setup_test_database().await.expect("a library");
    sqlx::raw_sql(OLD_TABLES)
        .execute(&pool)
        .await
        .expect("old tables");
    sqlx::raw_sql(planted).execute(&pool).await.expect("rows");
    sqlx::query(&format!("PRAGMA user_version = {}", LATEST - 1))
        .execute(&pool)
        .await
        .expect("a stamp");
    pool
}

async fn texts(pool: &SqlitePool, query: &str) -> Vec<String> {
    sqlx::query_scalar(query)
        .fetch_all(pool)
        .await
        .expect("a read")
}

#[tokio::test]
async fn off_becomes_the_switch_and_the_retired_rows_go() {
    for (mode, after) in [
        (r#""off""#, Some("false")),
        (r#""note""#, None),
        (r#""refuse""#, None),
    ] {
        let pool = before_step_7(&format!(
            "INSERT INTO settings_kv (key, value, updated_at) VALUES
                ('loop_guard_mode', '{mode}', 'then'),
                ('proxy_loop_detection', 'false', 'then'),
                ('auto_tune', 'true', 'then'),
                ('tool_call_floor', 'true', 'then'),
                ('agentic_sampling', 'false', 'then'),
                ('remote_api_key', '\"k\"', 'then'),
                ('remote_last_ticket', '\"t\"', 'then'),
                ('bind_host', '\"0.0.0.0\"', 'then'),
                ('default_download_path', '\"/m\"', 'then'),
                ('share_lan', 'true', 'then'),
                ('show_memory_fit_indicators', 'true', 'then'),
                ('header_facts_read', 'true', 'then'),
                ('proxy_port', '9191', 'then');"
        ))
        .await;

        run(&pool, LATEST).await.expect("step 7");

        let mut kept = vec!["header_facts_read".to_owned(), "proxy_port".to_owned()];
        if after.is_some() {
            kept.insert(1, "loop_guard".to_owned());
        }
        assert_eq!(
            texts(&pool, "SELECT key FROM settings_kv ORDER BY key").await,
            kept,
            "{mode}"
        );
        let guard: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings_kv WHERE key = 'loop_guard'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(guard.as_deref(), after, "{mode}");
    }
}

#[tokio::test]
async fn the_log_loses_its_mode_and_scans_that_met_are_added() {
    let pool = before_step_7(
        "INSERT INTO loop_guard_trips VALUES
            (7, 100, 'qwen', '0.33.0', 'refuse', 'loop', 'sig', 'ses', NULL, NULL),
            (9, 200, 'qwen', '0.33.0', 'note', 'stagnation', NULL, 'ses', 6, 5);
         INSERT INTO loop_guard_scans VALUES
            (20000, 'qwen', '0.33.0', 'refuse', 40),
            (20000, 'qwen', '0.33.0', 'note', 2),
            (20000, 'gemma', '0.33.0', 'note', 7),
            (20001, 'qwen', '0.33.0', 'note', 1);",
    )
    .await;

    run(&pool, LATEST).await.expect("step 7");

    for table in ["loop_guard_trips", "loop_guard_scans"] {
        let columns = texts(
            &pool,
            &format!("SELECT name FROM pragma_table_info('{table}') WHERE name = 'mode'"),
        )
        .await;
        assert!(columns.is_empty(), "{table} still has a mode");
    }
    assert_eq!(
        texts(
            &pool,
            "SELECT id || '|' || recorded_at || '|' || model_name || '|' || gglib_version \
             || '|' || detector || '|' || quote(signature_hash) || '|' || quote(session_hash) \
             || '|' || quote(repeat_count) || '|' || quote(threshold) \
             FROM loop_guard_trips ORDER BY id"
        )
        .await,
        [
            "7|100|qwen|0.33.0|loop|'sig'|'ses'|NULL|NULL",
            "9|200|qwen|0.33.0|stagnation|NULL|'ses'|6|5",
        ]
    );
    assert_eq!(
        texts(
            &pool,
            "SELECT day || '|' || model_name || '|' || gglib_version || '|' || scanned \
             FROM loop_guard_scans ORDER BY day, model_name"
        )
        .await,
        [
            "20000|gemma|0.33.0|7",
            "20000|qwen|0.33.0|42",
            "20001|qwen|0.33.0|1"
        ]
    );
    assert_eq!(
        texts(
            &pool,
            "SELECT name FROM pragma_index_list('loop_guard_trips') WHERE origin = 'c'"
        )
        .await,
        ["idx_loop_guard_trips_recorded_at"],
        "the index is back on the rebuilt table"
    );
}

#[tokio::test]
async fn a_stored_report_loses_the_proxy_arms_mode_and_no_other_byte() {
    let with_mode = r#"{"proxy":{"settings":{"tool_call_repair":true,"loop_guard_mode":"note"}}}"#;
    let without = r#"{ "proxy": null, "model_name": "m" }"#;
    let pool = before_step_7(&format!(
        "INSERT INTO models (name, file_path, added_at, model_key) \
             VALUES ('m', '/m.gguf', 'then', 'local:m');
         INSERT INTO benchmark_agentic_results \
             (model_id, raw_composite, gglib_composite, report_json, created_at) VALUES
             (1, 1, 1, '{with_mode}', 'then'),
             (1, 1, 1, '{without}', 'then'),
             (1, 1, 1, 'not json', 'then');"
    ))
    .await;

    run(&pool, LATEST).await.expect("step 7");

    assert_eq!(
        texts(
            &pool,
            "SELECT report_json FROM benchmark_agentic_results ORDER BY id"
        )
        .await,
        [
            r#"{"proxy":{"settings":{"tool_call_repair":true}}}"#,
            without,
            "not json"
        ]
    );
}
