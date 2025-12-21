//! Database test utilities.
//!
//! MIGRATION SHIM: Re-exports test database utilities from gglib-db.
//! This maintains API compatibility while the crate migration is in progress.

use anyhow::Result;
use gglib_db::TestDb;
use sqlx::SqlitePool;

/// Creates an in-memory `SQLite` database with the full production schema.
///
/// # Migration Note
/// This is a shim that wraps `gglib_db::TestDb`. It will be removed
/// once all tests are updated to use `TestDb` directly.
pub async fn setup_test_pool() -> Result<SqlitePool> {
    let test_db = TestDb::new().await?;
    Ok(test_db.pool().clone())
}
