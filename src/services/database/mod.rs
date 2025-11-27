#![doc = include_str!(concat!(env!("OUT_DIR"), "/services_database_docs.md"))]

mod error;
mod from_row;
mod models;
mod schema;
mod tags;

use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

// Re-export public types and functions
pub use error::ModelStoreError;
pub use models::{
    add_model, find_model_by_identifier, find_models_by_name, get_model_by_id, list_models,
    remove_model_by_id, update_model,
};
pub use schema::ensure_column_exists;
pub use tags::{add_model_tag, get_model_tags, get_models_by_tag, list_tags, remove_model_tag};

// Re-export create_schema for test infrastructure
// This allows tests to create in-memory databases with production-identical schema
pub use schema::create_schema;

/// Sets up the SQLite database connection and ensures the database schema exists.
///
/// This function:
/// 1. Creates the data directory if it doesn't exist
/// 2. Establishes a connection to the SQLite database file
/// 3. Creates all tables if they don't exist
/// 4. Runs any necessary schema migrations
///
/// # Returns
///
/// Returns a `Result<SqlitePool>` containing the database connection pool on success,
/// or an error if database setup fails.
///
/// # Errors
///
/// This function will return an error if:
/// - The data directory cannot be created
/// - The database file cannot be opened or created
/// - The table creation SQL fails to execute
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::services::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     Ok(())
/// }
/// ```
pub async fn setup_database() -> Result<SqlitePool> {
    // Get the absolute path to the database file
    let db_path = crate::utils::paths::get_database_path()?;

    let pool: SqlitePool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true),
    )
    .await?;

    // Create all tables and indexes
    create_schema(&pool).await?;

    // Initialize settings table (from settings service)
    crate::services::settings::init_settings_table(&pool).await?;

    Ok(pool)
}

/// Sets up an in-memory SQLite database for testing.
///
/// This creates a fresh in-memory database with the full production schema,
/// providing complete isolation between tests. Use this in unit tests instead
/// of `setup_database()` to avoid file system race conditions.
///
/// # Returns
///
/// Returns a `Result<SqlitePool>` containing the database connection pool.
#[cfg(test)]
pub async fn setup_test_database() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    create_schema(&pool).await?;
    crate::services::settings::init_settings_table(&pool).await?;
    Ok(pool)
}
