//! Database test utilities.
//!
//! Provides functions for creating isolated in-memory test databases
//! that match the production schema exactly.

use anyhow::Result;
use gglib::services::database;
use sqlx::SqlitePool;

/// Creates an in-memory SQLite database with the full production schema.
///
/// This function creates a fresh database for each test, ensuring complete
/// isolation between tests. The schema matches production exactly by reusing
/// the `create_schema()` function from the database module.
///
/// # Example
///
/// ```rust,ignore
/// use crate::common::database::setup_test_pool;
///
/// #[tokio::test]
/// async fn test_something() {
///     let pool = setup_test_pool().await.unwrap();
///     // Test with the pool...
/// }
/// ```
pub async fn setup_test_pool() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // Reuse the production schema to ensure parity
    database::create_schema(&pool).await?;

    Ok(pool)
}
