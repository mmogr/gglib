//! Composition utilities for building AppCore with SQLite backends.
//!
//! This module provides factory functions for wiring up the application
//! with SQLite repositories. It is focused purely on construction and
//! should not contain any domain logic.

/// Factory for creating AppCore instances with SQLite backends.
pub struct CoreFactory;

impl CoreFactory {
    /// Build an AppCore instance with SQLite storage.
    ///
    /// # Arguments
    ///
    /// * `db_path` - Path to the SQLite database file
    ///
    /// # Returns
    ///
    /// A fully configured AppCore instance ready for use.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let core = CoreFactory::build_sqlite("~/.gglib/gglib.db").await?;
    /// ```
    pub async fn build_sqlite(_db_path: &str) -> anyhow::Result<()> {
        // Placeholder - will be populated during extraction
        // let pool = create_pool(db_path).await?;
        // let model_repo = Arc::new(SqliteModelRepository::new(pool.clone()));
        // let settings_repo = Arc::new(SqliteSettingsRepository::new(pool.clone()));
        // Ok(AppCore::new(model_repo, settings_repo))
        todo!("CoreFactory::build_sqlite not yet implemented")
    }
}
