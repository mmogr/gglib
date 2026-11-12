//! The 0.33.0 fixture through step 7, with rows planted on the copy that the
//! fixture lacks: a guard switched off, two scans that differ only by mode,
//! and a report whose proxy arm names its mode. The fixture file itself is
//! never written.

use gglib_core::SettingsRepository;
use gglib_core::ports::NoopGgufParser;
use sqlx::SqlitePool;

use super::{FIXTURE, FIXTURE_GENERATION};
use crate::{SqliteSettingsRepository, backup_beside, setup_database};

/// The rows the fixture lacks, written as 0.33.0 would have.
const PLANTED: &str = r#"
UPDATE settings_kv SET value = '"off"' WHERE key = 'loop_guard_mode';
INSERT INTO loop_guard_scans (day, model_name, gglib_version, mode, scanned)
    VALUES (20769, 'Fixture Agent 0.6B', '0.33.0', 'note', 2);
UPDATE benchmark_agentic_results SET report_json = json_set(report_json,
    '$.proxy', json('{"settings":{"tool_call_repair":true,"loop_guard_mode":"note"}}'));
"#;

async fn scalars(pool: &SqlitePool, query: &str) -> Vec<String> {
    sqlx::query_scalar(query).fetch_all(pool).await.unwrap()
}

#[tokio::test]
async fn a_library_with_the_guard_off_keeps_it_off_and_loses_every_mode() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gglib.db");
    std::fs::copy(FIXTURE, &db).unwrap();
    let raw = SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    sqlx::raw_sql(PLANTED).execute(&raw).await.unwrap();
    raw.close().await;

    let pool = setup_database(&db, &NoopGgufParser).await.unwrap();

    assert_eq!(
        backup_beside(&db).map(|(generation, _)| generation),
        Some(FIXTURE_GENERATION),
        "the library was backed up before the upgrade"
    );
    let settings = SqliteSettingsRepository::new(pool.clone())
        .load()
        .await
        .unwrap();
    assert_eq!(settings.loop_guard, Some(false));
    assert!(
        scalars(
            &pool,
            "SELECT key FROM settings_kv WHERE key = 'loop_guard_mode'"
        )
        .await
        .is_empty()
    );
    assert!(
        scalars(
            &pool,
            "SELECT name FROM pragma_table_info('loop_guard_scans') WHERE name = 'mode' \
             UNION ALL SELECT name FROM pragma_table_info('loop_guard_trips') WHERE name = 'mode'"
        )
        .await
        .is_empty()
    );
    assert_eq!(
        scalars(
            &pool,
            "SELECT model_name || ' ' || scanned FROM loop_guard_scans ORDER BY day DESC"
        )
        .await,
        ["Fixture Agent 0.6B 42", "Fixture Plain 7"],
        "refuse's 40 and note's 2 for one day, model and version are one row"
    );
    assert_eq!(
        scalars(
            &pool,
            "SELECT json_extract(report_json, '$.proxy.settings') FROM benchmark_agentic_results"
        )
        .await,
        [r#"{"tool_call_repair":true}"#]
    );
    pool.close().await;
}
