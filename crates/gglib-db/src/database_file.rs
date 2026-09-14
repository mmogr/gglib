//! The database, and what `SQLite` keeps beside it, made private before
//! `SQLite` opens it.
//!
//! The database holds chat history, the proxy's API key and the environment
//! variables given to MCP servers, which is where their API keys go.

use std::io;
use std::path::Path;

use gglib_core::paths::{create_private_dir, create_private_file, make_private};

/// Make the database and its directory private to this user, before `SQLite`
/// opens the database.
///
/// Before, because of how `SQLite` sets modes. It gives a `-wal` or `-shm` it
/// creates the database's own mode, so a database created `0600` here has its
/// `-wal` and `-shm` `0600` from their first byte; a zero-length file is an
/// empty database to it. But it leaves alone the mode of a `-wal` or `-shm` it
/// finds with content in it, so any an older build left `0644` are tightened
/// here, along with the database and the directory.
///
/// # Errors
///
/// Whatever creating the directory or the database returns.
pub(crate) fn prepare(db_path: &Path) -> io::Result<()> {
    if let Some(dir) = db_path.parent() {
        create_private_dir(dir)?;
    }
    create_private_file(db_path)?;
    for suffix in ["-wal", "-shm"] {
        let mut sibling = db_path.as_os_str().to_owned();
        sibling.push(suffix);
        make_private(Path::new(&sibling));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::prepare;

    /// `prepare` leaves the database on disk, empty and `0600`, before any pool
    /// exists, so `SQLite` opens a file it did not create and gives its `-wal`
    /// and `-shm` that file's mode. That `setup_database` calls this before it
    /// connects is not visible to `tests/file_modes.rs`: tightening afterwards
    /// reaches the same modes. This pins what `prepare` leaves; the order rests
    /// on reading `setup_database`.
    #[test]
    fn prepare_leaves_an_empty_private_database_for_sqlite_to_open() {
        let root = tempfile::tempdir().expect("tempdir");
        let db = root.path().join("data").join("gglib.db");

        prepare(&db).expect("prepare");

        let meta = std::fs::metadata(&db).expect("the database is there before SQLite opens it");
        assert_eq!(meta.len(), 0);
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }
}
