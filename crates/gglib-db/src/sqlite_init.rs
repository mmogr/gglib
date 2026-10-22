//! `SQLite` initialises once, on one thread, before `setup_database` or
//! `setup_test_database` opens a pool.
//!
//! Every connection calls `sqlite3_initialize` as it opens, on the sqlx
//! worker thread that opens it, so pools opened together can enter it for
//! the first time together. One connection opened and closed here, behind a
//! process-wide cell, makes that first entry a single thread's; every pool
//! built after it finds `SQLite` already initialised.
//!
//! This is insurance, not a proven fix: whether threads entering
//! `sqlite3_initialize` together cause the crash in
//! <https://github.com/mmogr/gglib/issues/1064> is not established. The guard
//! covers only the pools built through `setup_database` and
//! `setup_test_database`.

use sqlx::{Connection, SqliteConnection};
use tokio::sync::OnceCell;

static INITIALISED: OnceCell<()> = OnceCell::const_new();

/// Opens and closes one in-memory connection until that succeeds once in this
/// process. A call made while one is in flight waits for it; a call after it
/// returns at once.
pub(crate) async fn ensure_initialised() -> sqlx::Result<()> {
    INITIALISED
        .get_or_try_init(|| async {
            SqliteConnection::connect("sqlite::memory:")
                .await?
                .close()
                .await
        })
        .await?;
    Ok(())
}

// The cell is process-wide, so each test passes in a full run whatever it
// calls: another test may have set the cell first. Run alone by name, each
// shows its setup function sets the cell.
#[cfg(test)]
mod tests {
    use super::INITIALISED;

    #[tokio::test]
    async fn setup_database_initialises_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        crate::setup_database(&dir.path().join("gglib.db"))
            .await
            .unwrap();
        assert!(INITIALISED.initialized());
    }

    #[tokio::test]
    async fn setup_test_database_initialises_sqlite() {
        crate::setup_test_database().await.unwrap();
        assert!(INITIALISED.initialized());
    }
}
