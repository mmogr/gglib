//! Unified application core service layer.
//!
//! This module provides a modular `AppCore` facade that serves as the single entry point
//! for all business logic, used by both CLI commands and GUI backends (Tauri/Web).
//!
//! # Architecture
//!
//! ```text
//!                         AppCore (Facade)
//!                              │
//!      ┌──────────┬────────────┼────────────┬────────────┐
//!      │          │            │            │            │
//!   Model     Server      Download     Settings      Proxy
//!   Service   Service     Service      Service      Service
//! ```
//!
//! # Design Principles
//!
//! - **Pool ownership**: `AppCore::new(pool)` — pool created at entry point, passed in
//! - **Pure services**: No interactive prompts; services accept complete data
//! - **Thin adapters**: CLI commands and GUI handlers delegate to AppCore
//!
//! # Example
//!
//! ```rust,no_run
//! use gglib::services::core::AppCore;
//! use gglib::services::database;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let pool = database::setup_database().await?;
//!     let core = AppCore::new(pool);
//!     
//!     // Use model service
//!     let models = core.models().list().await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod model_service;

use sqlx::SqlitePool;

pub use model_service::ModelService;

/// Unified application core providing access to all services.
///
/// `AppCore` is the central facade for all business logic in gglib.
/// It holds shared state (database pool) and provides access to
/// individual service modules.
#[derive(Clone)]
pub struct AppCore {
    db_pool: SqlitePool,
    model_service: ModelService,
}

impl AppCore {
    /// Create a new AppCore instance with the given database pool.
    ///
    /// The pool should be created at the application entry point
    /// (CLI main, GUI main, or test setup) and passed in.
    ///
    /// # Arguments
    ///
    /// * `db_pool` - A SQLite connection pool from `database::setup_database()`
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use gglib::services::core::AppCore;
    /// use gglib::services::database;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let pool = database::setup_database().await?;
    /// let core = AppCore::new(pool);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(db_pool: SqlitePool) -> Self {
        let model_service = ModelService::new(db_pool.clone());
        Self {
            db_pool,
            model_service,
        }
    }

    /// Get a reference to the database pool for custom operations.
    ///
    /// Prefer using the service methods when possible, but this
    /// provides escape hatch for operations not yet migrated.
    pub fn db_pool(&self) -> &SqlitePool {
        &self.db_pool
    }

    /// Access the model service for CRUD operations on GGUF models.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::AppCore;
    /// # async fn example(core: &AppCore) -> anyhow::Result<()> {
    /// let models = core.models().list().await?;
    /// for model in models {
    ///     println!("{}: {}", model.id.unwrap_or(0), model.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn models(&self) -> &ModelService {
        &self.model_service
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    #[tokio::test]
    async fn test_appcore_creation() {
        let pool = database::setup_database().await.unwrap();
        let core = AppCore::new(pool);
        // Just verify it doesn't panic
        let _ = core.db_pool();
        let _ = core.models();
    }
}
