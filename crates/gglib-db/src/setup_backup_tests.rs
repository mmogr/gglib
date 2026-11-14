//! Tests for [`super`]: which opens back a file up, what the backup holds,
//! what a failed one leaves, and what the refusal of a newer file says.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use super::{SCHEMA_GENERATION, backup_beside, backup_path, publish};
use crate::setup::setup_database;

async fn connect(path: &Path) -> SqlitePool {
    SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("open")
}

async fn scalar(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.expect(sql)
}

/// A file an older build left: stamped `generation`, with two rows in a
/// table of its own.
async fn older_file(path: &Path, generation: i64) {
    let pool = connect(path).await;
    for sql in [
        "CREATE TABLE schema_generation (generation INTEGER NOT NULL)",
        "CREATE TABLE kept (note TEXT)",
        "INSERT INTO kept (note) VALUES ('one'), ('two')",
    ] {
        sqlx::query(sql).execute(&pool).await.expect(sql);
    }
    sqlx::query("INSERT INTO schema_generation (generation) VALUES (?)")
        .bind(generation)
        .execute(&pool)
        .await
        .expect("stamp");
    pool.close().await;
}

async fn open(path: &Path) -> anyhow::Result<SqlitePool> {
    setup_database(path, &gglib_core::ports::NoopGgufParser).await
}

/// The names in `dir` other than the database and what `SQLite` keeps
/// beside it.
fn others(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read_dir")
        .map(|e| e.expect("entry").file_name().into_string().expect("utf-8"))
        .filter(|n| !["gglib.db", "gglib.db-wal", "gglib.db-shm"].contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

fn db_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("gglib.db")
}

/// Where this process writes `bak` before publishing it.
fn tmp_path(bak: &Path) -> PathBuf {
    let mut tmp = bak.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    PathBuf::from(tmp)
}

#[tokio::test]
async fn a_new_database_is_not_backed_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    open(&db_in(&dir)).await.expect("setup").close().await;
    assert_eq!(others(dir.path()), Vec::<String>::new());
}

#[tokio::test]
async fn a_database_at_this_generation_is_not_backed_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    open(&db).await.expect("setup").close().await;
    open(&db).await.expect("reopen").close().await;
    assert_eq!(others(dir.path()), Vec::<String>::new());
}

#[tokio::test]
async fn an_older_database_is_backed_up_as_it_was() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let older = SCHEMA_GENERATION - 1;
    older_file(&db, older).await;

    open(&db).await.expect("an older file opens").close().await;

    let bak = backup_path(&db, older);
    assert_eq!(backup_beside(&db), Some((older, bak.clone())));
    assert_eq!(others(dir.path()), vec![format!("gglib.db.gen{older}.bak")]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&bak).expect("bak").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let copy = connect(&bak).await;
    assert_eq!(scalar(&copy, "SELECT COUNT(*) FROM kept").await, 2);
    assert_eq!(
        scalar(&copy, "SELECT generation FROM schema_generation").await,
        older
    );
}

#[tokio::test]
async fn a_database_from_before_the_generation_is_backed_up_as_gen0() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let pool = connect(&db).await;
    sqlx::query("CREATE TABLE kept (note TEXT)")
        .execute(&pool)
        .await
        .expect("table");
    pool.close().await;

    open(&db)
        .await
        .expect("an unstamped file opens")
        .close()
        .await;

    assert_eq!(others(dir.path()), vec!["gglib.db.gen0.bak".to_owned()]);
}

#[tokio::test]
async fn a_new_backup_removes_an_older_generations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let older = SCHEMA_GENERATION - 1;
    std::fs::write(backup_path(&db, older - 1), "an older backup").expect("write");
    older_file(&db, older).await;

    open(&db).await.expect("an older file opens").close().await;

    assert_eq!(others(dir.path()), vec![format!("gglib.db.gen{older}.bak")]);
    let copy = connect(&backup_path(&db, older)).await;
    assert_eq!(scalar(&copy, "SELECT COUNT(*) FROM kept").await, 2);
}

#[tokio::test]
async fn a_backup_of_the_same_generation_is_kept_and_no_copy_is_taken() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let older = SCHEMA_GENERATION - 1;
    let bak = backup_path(&db, older);
    std::fs::write(&bak, "the first backup").expect("write");
    let modified = std::fs::metadata(&bak)
        .expect("bak")
        .modified()
        .expect("mtime");
    // A directory where the temporary copy goes, so taking one fails the open.
    let tmp = tmp_path(&bak);
    std::fs::create_dir(&tmp).expect("mkdir");
    older_file(&db, older).await;

    open(&db).await.expect("an older file opens").close().await;

    assert_eq!(std::fs::read(&bak).expect("read"), b"the first backup");
    assert_eq!(
        std::fs::metadata(&bak)
            .expect("bak")
            .modified()
            .expect("mtime"),
        modified
    );
}

#[test]
fn a_backup_published_first_by_another_process_is_kept() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (tmp, bak) = (dir.path().join("copy.tmp"), dir.path().join("copy.bak"));
    std::fs::write(&tmp, "ours").expect("write");
    std::fs::write(&bak, "theirs").expect("write");

    publish(&tmp, &bak).expect("a backup already there is not a failure");

    assert_eq!(std::fs::read(&bak).expect("read"), b"theirs");
    std::fs::remove_file(&bak).expect("remove");
    publish(&tmp, &bak).expect("publish");
    assert_eq!(std::fs::read(&bak).expect("read"), b"ours");
    let missing = dir.path().join("missing.tmp");
    publish(&missing, &dir.path().join("other.bak")).expect_err("any other failure is one");
}

#[tokio::test]
async fn a_temporary_file_left_by_this_process_id_is_written_over() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let older = SCHEMA_GENERATION - 1;
    let bak = backup_path(&db, older);
    std::fs::write(tmp_path(&bak), "left by a crash").expect("write");
    older_file(&db, older).await;

    open(&db).await.expect("an older file opens").close().await;

    assert_eq!(others(dir.path()), vec![format!("gglib.db.gen{older}.bak")]);
    let copy = connect(&bak).await;
    assert_eq!(scalar(&copy, "SELECT COUNT(*) FROM kept").await, 2);
}

#[tokio::test]
async fn a_failed_backup_fails_the_open_and_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    let older = SCHEMA_GENERATION - 1;
    older_file(&db, older).await;
    // A directory where the temporary copy goes, so taking one fails.
    let tmp = tmp_path(&backup_path(&db, older));
    std::fs::create_dir(&tmp).expect("mkdir");

    let err = open(&db).await.expect_err("the open must fail");
    assert!(
        format!("{err:#}").contains("nothing has been changed"),
        "{err:#}"
    );

    let pool = connect(&db).await;
    assert_eq!(
        scalar(&pool, "SELECT generation FROM schema_generation").await,
        older
    );
    assert_eq!(scalar(&pool, "PRAGMA user_version").await, 0);
    assert_eq!(scalar(&pool, "SELECT COUNT(*) FROM sqlite_master").await, 2);
    // No backup, and nothing beside the file but the directory put there.
    let tmp_name = tmp.file_name().and_then(|n| n.to_str()).expect("utf-8");
    assert_eq!(others(dir.path()), vec![tmp_name.to_owned()]);
}

#[tokio::test]
async fn the_refusal_of_a_newer_database_says_how_to_restore_its_backup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    older_file(&db, SCHEMA_GENERATION + 1).await;
    let bak = backup_path(&db, SCHEMA_GENERATION);
    std::fs::write(&bak, "").expect("write");

    let err = open(&db)
        .await
        .expect_err("a newer file is refused")
        .to_string();

    assert!(err.contains(&bak.display().to_string()), "{err}");
    assert!(
        err.contains(", taken "),
        "says when the copy was taken: {err}"
    );
    #[cfg(unix)]
    {
        let (db, bak) = (db.display(), bak.display());
        let restore = format!("rm -f '{db}-wal' '{db}-shm' && mv '{bak}' '{db}'");
        assert!(err.contains(&restore), "{err}");
    }
}

#[tokio::test]
async fn the_refusal_names_no_backup_this_build_cannot_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    older_file(&db, SCHEMA_GENERATION + 5).await;
    let bak = backup_path(&db, SCHEMA_GENERATION + 4);
    std::fs::write(&bak, "").expect("write");

    let err = open(&db)
        .await
        .expect_err("a newer file is refused")
        .to_string();

    assert!(err.contains("newer gglib"), "{err}");
    assert!(!err.contains(".bak"), "{err}");
}

/// A directory where the backup goes is no backup: the open fails before
/// anything changes, and no refusal offers it.
#[tokio::test]
async fn a_directory_at_the_backups_name_fails_the_open_and_is_never_offered() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    older_file(&db, SCHEMA_GENERATION - 1).await;
    let in_the_way = backup_path(&db, SCHEMA_GENERATION - 1);
    std::fs::create_dir(&in_the_way).expect("mkdir");

    let err = open(&db).await.expect_err("no backup, no upgrade");

    assert!(
        format!("{err:#}").contains("nothing has been changed"),
        "{err:#}"
    );
    assert!(backup_beside(&db).is_none());
}

#[test]
fn backup_beside_takes_the_highest_backup_named_as_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = db_in(&dir);
    for name in [
        "gglib.db.gen.bak",
        "gglib.db.gen+5.bak",
        "gglib.db.gen5.bak.123.tmp",
        "other.db.gen5.bak",
    ] {
        std::fs::write(dir.path().join(name), "").expect("write");
    }
    assert_eq!(backup_beside(&db), None);

    for generation in [3, 2] {
        std::fs::write(backup_path(&db, generation), "").expect("write");
    }
    assert_eq!(backup_beside(&db), Some((3, backup_path(&db, 3))));
}
