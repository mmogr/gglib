//! The fold of the loop guard's switch into its mode, on database files of
//! the test's own: what each stored pair answers afterwards, that it runs
//! once, and what it writes. `setup_settings_unreadable_tests.rs` has the
//! records it must leave alone.

use std::path::{Path, PathBuf};

use gglib_core::LoopGuardMode::{self, Note, Off, Refuse};
use gglib_core::SettingsRepository;

use super::*;
use crate::setup::CANONICAL_PATH_SCHEMA_VERSION;
use crate::{SqliteSettingsRepository, setup_database};

/// A settings row, byte for byte: its key, its value's type and bytes, and
/// when it was written.
pub(super) type StoredRow = (String, String, Vec<u8>, String);

/// The row `key` has when it holds the text `value`, written at `at`.
pub(super) fn text_row(key: &str, value: &str, at: &str) -> StoredRow {
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
pub(super) async fn left_by_an_older_build(
    path: &Path,
    version: i64,
    rows: &[(&str, &str)],
) -> PathBuf {
    let pool = setup_database(path).await.expect("a fresh database");
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

pub(super) async fn stamp(pool: &SqlitePool, version: i64) {
    sqlx::query(&format!("PRAGMA user_version = {version}"))
        .execute(pool)
        .await
        .expect("a stamp");
}

/// Every settings row, in key order.
pub(super) async fn rows(pool: &SqlitePool) -> Vec<StoredRow> {
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

/// What the loop guard does, as this build loads the record.
async fn answer(pool: &SqlitePool) -> LoopGuardMode {
    SqliteSettingsRepository::new(pool.clone())
        .load()
        .await
        .expect("a record this build reads")
        .effective_loop_guard_mode()
}

/// What a row holds, or `None` for no row.
type Held = Option<&'static str>;

const OFF: Held = Some(r#""off""#);
const NOTE: Held = Some(r#""note""#);
const REFUSE: Held = Some(r#""refuse""#);
const NULL: Held = Some("null");

/// Each stored switch and mode, what the build before the fold answered for
/// the pair, and the mode's row once folded.
///
/// The answers are that build's rule written out, not computed: a stored mode
/// answered, whatever the switch said; with none, `false` answered `off` and
/// `true` or no switch answered `note`. A JSON `null` in either row read as no
/// row. Only a `false` switch with no mode moves the mode's row; every other
/// pair leaves it as stored.
const PAIRS: &[(Held, Held, LoopGuardMode, Held)] = &[
    (None, None, Note, None),
    (None, OFF, Off, OFF),
    (None, NOTE, Note, NOTE),
    (None, REFUSE, Refuse, REFUSE),
    (Some("true"), None, Note, None),
    (Some("true"), OFF, Off, OFF),
    (Some("true"), NOTE, Note, NOTE),
    (Some("true"), REFUSE, Refuse, REFUSE),
    (Some("false"), None, Off, OFF),
    (Some("false"), OFF, Off, OFF),
    (Some("false"), NOTE, Note, NOTE),
    (Some("false"), REFUSE, Refuse, REFUSE),
    (NULL, None, Note, None),
    (NULL, OFF, Off, OFF),
    (Some("true"), NULL, Note, NULL),
    (Some("false"), NULL, Off, OFF),
];

#[tokio::test]
async fn every_stored_pair_answers_once_folded_what_it_answered_before() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (case, (switch, mode, answered, mode_after)) in PAIRS.iter().enumerate() {
        let stored: Vec<(&str, &str)> = [(SWITCH, switch), (MODE, mode)]
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect();
        let path = dir.path().join(format!("{case}.db"));
        left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, &stored).await;

        let pool = setup_database(&path).await.expect("the boot");

        let pair = format!("switch {switch:?}, mode {mode:?}");
        assert_eq!(answer(&pool).await, *answered, "{pair}");
        assert_eq!(value_of(&pool, SWITCH).await, None, "{pair}");
        assert_eq!(
            value_of(&pool, MODE).await.as_deref(),
            *mode_after,
            "{pair}"
        );
        pool.close().await;
    }
}

/// A library from before the first stamp takes both steps in one boot.
#[tokio::test]
async fn a_database_never_stamped_is_folded_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = left_by_an_older_build(&dir.path().join("gglib.db"), 0, &[(SWITCH, "false")]).await;

    let pool = setup_database(&path).await.expect("the boot");

    assert_eq!(answer(&pool).await, Off);
    assert_eq!(
        user_version(&pool).await.expect("a version"),
        LOOP_GUARD_MODE_SCHEMA_VERSION
    );
}

/// The stamp is what makes it once: a switch written into a stamped database,
/// as an older build's `--proxy-loop-detection false` would write it, is a
/// row no field answers to and is left where it is.
#[tokio::test]
async fn the_fold_runs_once_and_a_switch_written_after_it_is_not_folded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let pool = setup_database(&path).await.expect("a fresh database");
    assert_eq!(
        user_version(&pool).await.expect("a version"),
        LOOP_GUARD_MODE_SCHEMA_VERSION,
        "a fresh database is stamped as it is made"
    );
    plant(&pool, SWITCH, "false").await;
    pool.close().await;

    let pool = setup_database(&path).await.expect("the second boot");

    assert_eq!(rows(&pool).await, [text_row(SWITCH, "false", "then")]);
    assert_eq!(answer(&pool).await, Note);
}

/// A second pass over a folded record writes nothing: the mode's row keeps
/// the time the test gave it, so it was not written again with the same value.
#[tokio::test]
async fn a_second_pass_over_a_folded_record_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, &[(SWITCH, "false")]).await;
    let pool = setup_database(&path).await.expect("the boot");
    sqlx::query("UPDATE settings_kv SET updated_at = 'folded'")
        .execute(&pool)
        .await
        .expect("a marked row");
    let folded = [text_row(MODE, r#""off""#, "folded")];
    assert_eq!(rows(&pool).await, folded);
    stamp(&pool, CANONICAL_PATH_SCHEMA_VERSION).await;

    fold_loop_guard_switch_into_mode(&pool)
        .await
        .expect("the second pass");

    assert_eq!(rows(&pool).await, folded);
    assert_eq!(
        user_version(&pool).await.expect("a version"),
        LOOP_GUARD_MODE_SCHEMA_VERSION
    );
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
async fn the_fold_writes_the_mode_and_the_switch_and_no_other_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let stored = [OTHERS, &[(SWITCH, "false")]].concat();
    left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, &stored).await;

    let pool = setup_database(&path).await.expect("the boot");

    let (mode, others): (Vec<_>, Vec<_>) = rows(&pool)
        .await
        .into_iter()
        .partition(|(key, ..)| key == MODE);
    let kept: Vec<_> = OTHERS
        .iter()
        .map(|(key, value)| text_row(key, value, "then"))
        .collect();
    assert_eq!(others, kept);
    let [(_, kind, value, written)] = mode.as_slice() else {
        panic!("one row for the mode, found {mode:?}");
    };
    assert_eq!(
        (kind.as_str(), value.as_slice()),
        ("text", &br#""off""#[..])
    );
    assert_ne!(written, "then", "the mode's row is new");
}

/// A stored mode already answers for the pair, so its row is not a write
/// either: it keeps its bytes, padding included, and its time.
#[tokio::test]
async fn a_stored_mode_keeps_its_row_byte_for_byte_when_the_switch_goes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let mode = (MODE, r#" "refuse" "#);
    let stored = [OTHERS, &[mode, (SWITCH, "false")]].concat();
    left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, &stored).await;

    let pool = setup_database(&path).await.expect("the boot");

    let mut kept: Vec<_> = [OTHERS, &[mode]]
        .concat()
        .iter()
        .map(|(key, value)| text_row(key, value, "then"))
        .collect();
    kept.sort();
    assert_eq!(rows(&pool).await, kept);
    assert_eq!(answer(&pool).await, Refuse);
}

#[tokio::test]
async fn the_auto_tune_row_is_reclaimed_as_a_database_opens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let stored = [("auto_tune", "true"), ("proxy_port", "9191")];
    left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, &stored).await;

    let pool = setup_database(&path).await.expect("the boot");

    assert_eq!(rows(&pool).await, [text_row("proxy_port", "9191", "then")]);
}
