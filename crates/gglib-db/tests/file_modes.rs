//! The database, the `-wal` and `-shm` beside it, and the directory they sit
//! in are this user's alone.
//!
//! Through the public `setup_database` on a real directory, as a caller opens
//! it: the `-wal` and `-shm` are `SQLite`'s to create, and only a real open
//! shows what mode it gives them.
//!
//! Each check bites under the usual `022` umask. Under `077` a plain create is
//! private too and these cannot tell the two apart; the umask is process-wide
//! and the tests run in parallel, so it is not set here to force the question.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use gglib_db::setup_database;

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The database and the `-wal` and `-shm` `SQLite` keeps beside it.
fn files_of(db: &Path) -> [PathBuf; 3] {
    let beside = |suffix: &str| {
        let mut name = db.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    };
    [db.to_path_buf(), beside("-wal"), beside("-shm")]
}

#[tokio::test]
async fn the_database_is_not_readable_by_other_users() {
    let root = tempfile::tempdir().expect("tempdir");
    let db = root.path().join("data").join("gglib.db");

    let pool = setup_database(&db).await.expect("setup");
    // Held, because the last connection to close removes the `-wal` and `-shm`.
    let _held = pool.acquire().await.expect("connection");

    for file in files_of(&db) {
        let mode = mode(&file);
        assert_eq!(mode & 0o077, 0, "{}: {mode:o}", file.display());
    }
}

#[tokio::test]
async fn the_data_directory_is_not_listable_by_other_users() {
    let root = tempfile::tempdir().expect("tempdir");
    let data = root.path().join("data");

    setup_database(&data.join("gglib.db")).await.expect("setup");

    assert_eq!(mode(&data) & 0o077, 0, "{:o}", mode(&data));
}

/// What an older build left: everything readable, and a `-wal` and `-shm`
/// with content in them, as they have while a daemon holds the database open.
/// `SQLite` sets the mode of a `-wal` or `-shm` it creates or finds empty and
/// never of one with content, so these are gglib's to tighten.
#[tokio::test]
async fn a_database_an_older_build_left_readable_is_tightened_with_its_wal_and_shm() {
    let root = tempfile::tempdir().expect("tempdir");
    let data = root.path().join("data");
    let db = data.join("gglib.db");
    let daemon = setup_database(&db).await.expect("the first open");
    let _held = daemon.acquire().await.expect("connection");
    set_mode(&data, 0o755);
    for file in files_of(&db) {
        assert!(
            fs::metadata(&file).expect("metadata").len() > 0,
            "{}",
            file.display()
        );
        set_mode(&file, 0o644);
    }

    let _pool = setup_database(&db).await.expect("the second open");

    assert_eq!(mode(&data), 0o700, "{}", data.display());
    for file in files_of(&db) {
        assert_eq!(mode(&file), 0o600, "{}", file.display());
    }
}
