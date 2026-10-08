//! The records the fold must leave alone: one the build before it could not
//! read answered nothing, so there is nothing to carry over, and a boot that
//! failed on it would take the whole database with it (#1141 is the settings
//! commands failing on such a record; this must not add the boot to them).

use gglib_core::SettingsRepository;

use super::tests::{left_by_an_older_build, rows, stamp, text_row};
use super::*;
use crate::setup::CANONICAL_PATH_SCHEMA_VERSION;
use crate::{SqliteSettingsRepository, setup_database};

/// Each with a switch that says `off`, or that a looser reading would take
/// to, so a pass that folded any of them would show in the rows.
const UNREADABLE: &[(&str, &[(&str, &str)])] = &[
    ("a switch that is a string", &[(SWITCH, r#""false""#)]),
    ("a switch that is a number", &[(SWITCH, "0")]),
    ("a switch that is not JSON", &[(SWITCH, "fals")]),
    (
        "a mode a later build wrote",
        &[(SWITCH, "false"), (MODE, r#""strict""#)],
    ),
    (
        "a mode that is not JSON",
        &[(SWITCH, "false"), (MODE, "off")],
    ),
    (
        "another row that is not its field's type",
        &[(SWITCH, "false"), ("proxy_port", r#""abc""#)],
    ),
    (
        "another row that is not JSON",
        &[(SWITCH, "false"), ("bind_host", "0.0.0.0")],
    ),
];

#[tokio::test]
async fn a_record_that_cannot_be_read_is_left_as_found_and_the_boot_goes_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (case, (what, stored)) in UNREADABLE.iter().enumerate() {
        let path = dir.path().join(format!("{case}.db"));
        left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, stored).await;
        let mut found: Vec<_> = stored
            .iter()
            .map(|(key, value)| text_row(key, value, "then"))
            .collect();
        found.sort();

        let pool = setup_database(&path)
            .await
            .unwrap_or_else(|refused| panic!("{what}: the boot failed: {refused:#}"));

        assert_eq!(rows(&pool).await, found, "{what}");
        assert_eq!(
            user_version(&pool).await.expect("a version"),
            LOOP_GUARD_MODE_SCHEMA_VERSION,
            "{what}: stamped all the same, so no later boot tries again"
        );
        pool.close().await;
    }
}

/// The fixture is held to its word. A switch that is JSON and not a boolean
/// is the one kind of record here this build loads, the switch being a row no
/// field answers to now; every other is one its own load refuses.
#[tokio::test]
async fn all_but_the_switches_that_are_json_are_records_this_build_cannot_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (case, (what, stored)) in UNREADABLE.iter().enumerate() {
        let path = dir.path().join(format!("{case}.db"));
        left_by_an_older_build(&path, CANONICAL_PATH_SCHEMA_VERSION, stored).await;
        let pool = setup_database(&path).await.expect("the boot");

        let loaded = SqliteSettingsRepository::new(pool.clone()).load().await;

        assert_eq!(loaded.is_ok(), case < 2, "{what} loaded as {loaded:?}");
        pool.close().await;
    }
}

/// A value that is not text is not read as the text it spells. These bytes
/// spell `false`, and the row is left as it is.
#[tokio::test]
async fn a_switch_stored_as_a_blob_is_left_as_found_and_the_boot_goes_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let pool = setup_database(&path).await.expect("a fresh database");
    sqlx::query("INSERT INTO settings_kv (key, value, updated_at) VALUES (?, ?, 'then')")
        .bind(SWITCH)
        .bind(&b"false"[..])
        .execute(&pool)
        .await
        .expect("a planted row");
    stamp(&pool, CANONICAL_PATH_SCHEMA_VERSION).await;
    pool.close().await;

    let pool = setup_database(&path).await.expect("the boot");

    let blob = (
        SWITCH.to_owned(),
        "blob".to_owned(),
        b"false".to_vec(),
        "then".to_owned(),
    );
    assert_eq!(rows(&pool).await, [blob]);
}

/// The version is asked again under the lock: a database another process
/// stamped while this one waited is not folded, and a later stamp is not
/// lowered to this one.
#[tokio::test]
async fn a_database_stamped_before_the_lock_was_held_is_neither_folded_nor_restamped() {
    const LATER: i64 = LOOP_GUARD_MODE_SCHEMA_VERSION + 1;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    left_by_an_older_build(&path, LATER, &[(SWITCH, "false")]).await;
    let pool = setup_database(&path).await.expect("the boot");

    fold_and_stamp(&pool).await.expect("the pass");

    assert_eq!(rows(&pool).await, [text_row(SWITCH, "false", "then")]);
    assert_eq!(user_version(&pool).await.expect("a version"), LATER);
}
