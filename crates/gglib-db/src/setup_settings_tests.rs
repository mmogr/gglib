//! The fold of the loop guard's switch into its mode (step 2), on database
//! files of the test's own: what each stored pair answers once a boot has run
//! it and step 6 after it, that it runs once, and what it writes.

use std::path::{Path, PathBuf};

use gglib_core::SettingsRepository;

use sqlx::SqlitePool;

use super::super::migrations::{LATEST, run, user_version};
use crate::{SqliteSettingsRepository, setup_database};

/// The key of the row that was the loop guard's switch.
const SWITCH: &str = "proxy_loop_detection";

/// The key of the row that held the loop guard's mode.
const MODE: &str = "loop_guard_mode";

/// The key of the row that holds the loop guard's switch today.
const GUARD: &str = "loop_guard";

/// The version a build before the fold left: step 1 done.
const BEFORE_THE_FOLD: i64 = 1;

/// A settings row, byte for byte: its key, its value's type and bytes, and
/// when it was written.
type StoredRow = (String, String, Vec<u8>, String);

/// The row `key` has when it holds the text `value`, written at `at`.
fn text_row(key: &str, value: &str, at: &str) -> StoredRow {
    (
        key.to_owned(),
        "text".to_owned(),
        value.as_bytes().to_vec(),
        at.to_owned(),
    )
}

/// A database file as a build before the fold left it: this build's schema,
/// `version` for its `user_version`, and `rows` in its settings table, each
/// written at `then`.
async fn left_by_an_older_build(path: &Path, version: i64, rows: &[(&str, &str)]) -> PathBuf {
    let pool = setup_database(path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("a fresh database");
    for (key, value) in rows {
        plant(&pool, key, value).await;
    }
    stamp(&pool, version).await;
    pool.close().await;
    path.to_path_buf()
}

async fn plant(pool: &SqlitePool, key: &str, value: &str) {
    sqlx::query("INSERT INTO settings_kv (key, value, updated_at) VALUES (?, ?, 'then')")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .expect("a planted row");
}

async fn stamp(pool: &SqlitePool, version: i64) {
    sqlx::query(&format!("PRAGMA user_version = {version}"))
        .execute(pool)
        .await
        .expect("a stamp");
}

/// Every settings row, in key order.
async fn rows(pool: &SqlitePool) -> Vec<StoredRow> {
    sqlx::query_as(
        "SELECT key, typeof(value), CAST(value AS BLOB), updated_at \
         FROM settings_kv ORDER BY key",
    )
    .fetch_all(pool)
    .await
    .expect("the rows")
}

async fn value_of(pool: &SqlitePool, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings_kv WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .expect("a row or none")
}

/// Whether the loop guard is on, as this build loads the record.
async fn answer(pool: &SqlitePool) -> bool {
    SqliteSettingsRepository::new(pool.clone())
        .load()
        .await
        .expect("a record this build reads")
        .effective_loop_guard()
}

/// What a row holds, or `None` for no row.
type Held = Option<&'static str>;

const OFF: Held = Some(r#""off""#);
const NOTE: Held = Some(r#""note""#);
const REFUSE: Held = Some(r#""refuse""#);
const NULL: Held = Some("null");

/// Each stored switch and mode, whether the guard is on once both steps ran,
/// and the switch's row today.
///
/// The build before the fold answered a stored mode whatever the switch
/// said; with none, `false` answered `off` and `true` or no switch answered
/// `note`. A JSON `null` in either row read as no row. Only `off` switches
/// the guard off today; `note` and `refuse` are on, with no row.
const PAIRS: &[(Held, Held, bool, Held)] = &[
    (None, None, true, None),
    (None, OFF, false, Some("false")),
    (None, NOTE, true, None),
    (None, REFUSE, true, None),
    (Some("true"), None, true, None),
    (Some("true"), OFF, false, Some("false")),
    (Some("true"), NOTE, true, None),
    (Some("true"), REFUSE, true, None),
    (Some("false"), None, false, Some("false")),
    (Some("false"), OFF, false, Some("false")),
    (Some("false"), NOTE, true, None),
    (Some("false"), REFUSE, true, None),
    (NULL, None, true, None),
    (NULL, OFF, false, Some("false")),
    (Some("true"), NULL, true, None),
    (Some("false"), NULL, false, Some("false")),
];

#[tokio::test]
async fn every_stored_pair_answers_what_it_answered_before() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (case, (switch, mode, on, guard_after)) in PAIRS.iter().enumerate() {
        let stored: Vec<(&str, &str)> = [(SWITCH, switch), (MODE, mode)]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect();
        let path = dir.path().join(format!("{case}.db"));
        left_by_an_older_build(&path, BEFORE_THE_FOLD, &stored).await;

        let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
            .await
            .expect("the boot");

        let pair = format!("switch {switch:?}, mode {mode:?}");
        assert_eq!(answer(&pool).await, *on, "{pair}");
        assert_eq!(value_of(&pool, SWITCH).await, None, "{pair}");
        assert_eq!(value_of(&pool, MODE).await, None, "{pair}");
        assert_eq!(
            value_of(&pool, GUARD).await.as_deref(),
            *guard_after,
            "{pair}"
        );
        pool.close().await;
    }
}

/// A library from before the first stamp takes every step in one boot.
#[tokio::test]
async fn a_database_never_stamped_is_folded_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = left_by_an_older_build(&dir.path().join("gglib.db"), 0, &[(SWITCH, "false")]).await;

    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("the boot");

    assert!(!answer(&pool).await);
    assert_eq!(user_version(&pool).await.expect("a version"), LATEST);
}

/// The stamp is what makes it once: a switch written into a stamped database
/// is a row no field answers to and is left where it is.
#[tokio::test]
async fn the_fold_runs_once_and_a_switch_written_after_it_is_not_folded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("a fresh database");
    assert_eq!(
        user_version(&pool).await.expect("a version"),
        LATEST,
        "a fresh database is stamped as it is made"
    );
    plant(&pool, SWITCH, "false").await;
    pool.close().await;

    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("the second boot");

    assert_eq!(rows(&pool).await, [text_row(SWITCH, "false", "then")]);
    assert!(answer(&pool).await);
}

/// A second pass over a folded record writes nothing: the switch's row keeps
/// the time the test gave it, so it was not written again with the same value.
#[tokio::test]
async fn a_second_pass_over_a_folded_record_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    left_by_an_older_build(&path, BEFORE_THE_FOLD, &[(SWITCH, "false")]).await;
    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("the boot");
    sqlx::query("UPDATE settings_kv SET updated_at = 'folded'")
        .execute(&pool)
        .await
        .expect("a marked row");
    let folded = [text_row(GUARD, "false", "folded")];
    assert_eq!(rows(&pool).await, folded);
    stamp(&pool, BEFORE_THE_FOLD).await;

    run(&pool, LATEST).await.expect("the second pass");

    assert_eq!(rows(&pool).await, folded);
    assert_eq!(user_version(&pool).await.expect("a version"), LATEST);
}

/// Values no writer of this build would produce byte for byte, a key no field
/// answers to among them: a pass that read the record and wrote it back whole
/// would move every one. In key order, as [`rows`] reports them.
const OTHERS: &[(&str, &str)] = &[
    ("a_setting_of_a_later_build", r#"{ "kept" : true }"#),
    ("inference_profiles", "[ ]"),
    ("max_stagnation_steps", " 7"),
    ("proxy_port", "9191 "),
];

#[tokio::test]
async fn the_steps_write_the_switch_and_no_other_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let stored = [OTHERS, &[(SWITCH, "false")]].concat();
    left_by_an_older_build(&path, BEFORE_THE_FOLD, &stored).await;

    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("the boot");

    let mut kept: Vec<_> = [OTHERS, &[(GUARD, "false")]]
        .concat()
        .iter()
        .map(|(key, value)| text_row(key, value, "then"))
        .collect();
    kept.sort();
    assert_eq!(rows(&pool).await, kept);
}

/// A stored mode that is not `off` leaves the guard on, with no row.
#[tokio::test]
async fn a_stored_refuse_turns_into_a_guard_that_is_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let stored = [OTHERS, &[(MODE, r#""refuse""#), (SWITCH, "false")]].concat();
    left_by_an_older_build(&path, BEFORE_THE_FOLD, &stored).await;

    let pool = setup_database(&path, &gglib_core::ports::NoopGgufParser)
        .await
        .expect("the boot");

    let kept: Vec<_> = OTHERS
        .iter()
        .map(|(key, value)| text_row(key, value, "then"))
        .collect();
    assert_eq!(rows(&pool).await, kept);
    assert!(answer(&pool).await);
}
