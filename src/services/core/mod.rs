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

pub mod download_service;
pub mod model_service;
pub mod proxy_service;
pub mod server_service;
pub mod settings_service;

use crate::services::process_manager::ProcessManager;
use crate::utils::paths::get_llama_server_path;
use sqlx::SqlitePool;
use std::sync::Arc;

pub use download_service::DownloadService;
pub use model_service::ModelService;
pub use proxy_service::ProxyService;
pub use server_service::{ServerService, StartServerConfig, StartServerResult};
pub use settings_service::{ModelsDirectoryInfo, SettingsService};

/// Unified application core providing access to all services.
///
/// `AppCore` is the central facade for all business logic in gglib.
/// It holds shared state (database pool) and provides access to
/// individual service modules.
pub struct AppCore {
    db_pool: SqlitePool,
    model_service: ModelService,
    server_service: ServerService,
    proxy_service: ProxyService,
    download_service: DownloadService,
    settings_service: SettingsService,
}

impl AppCore {
    /// Create a new AppCore instance with the given database pool.
    ///
    /// Uses default settings for server management:
    /// - Base port: 9000
    /// - Max concurrent servers: 5
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
        Self::with_config(db_pool, 9000, 5)
    }

    /// Create a new AppCore with custom server configuration.
    ///
    /// # Arguments
    ///
    /// * `db_pool` - A SQLite connection pool
    /// * `base_port` - Base port for llama-server instances
    /// * `max_concurrent` - Maximum number of concurrent servers
    pub fn with_config(db_pool: SqlitePool, base_port: u16, max_concurrent: usize) -> Self {
        let model_service = ModelService::new(db_pool.clone());

        // Create shared ProcessManager for server service
        let llama_server_path = get_llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string());

        let process_manager = Arc::new(ProcessManager::new_concurrent(
            base_port,
            max_concurrent,
            llama_server_path,
        ));

        let server_service = ServerService::new(process_manager, model_service.clone());
        let proxy_service = ProxyService::new(db_pool.clone());
        let download_service = DownloadService::new();
        let settings_service = SettingsService::new(db_pool.clone());

        Self {
            db_pool,
            model_service,
            server_service,
            proxy_service,
            download_service,
            settings_service,
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

    /// Access the server service for managing llama-server instances.
    ///
    /// This is used for GUI-style background server management.
    /// For CLI foreground serving, use `commands::serve` directly.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::{AppCore, StartServerConfig};
    /// # async fn example(core: &AppCore) -> anyhow::Result<()> {
    /// // Start a server
    /// let result = core.servers().start(StartServerConfig {
    ///     model_id: 1,
    ///     context_length: Some(4096),
    ///     jinja: None,
    /// }).await?;
    /// println!("Server running on port {}", result.port);
    ///
    /// // List running servers
    /// let servers = core.servers().list().await;
    /// # Ok(())
    /// # }
    /// ```
    pub fn servers(&self) -> &ServerService {
        &self.server_service
    }

    /// Access the proxy service for OpenAI-compatible proxy management.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::AppCore;
    /// # async fn example(core: &AppCore) -> anyhow::Result<()> {
    /// // Start proxy
    /// core.proxy().start("127.0.0.1".to_string(), 8080, 5500, 4096).await?;
    ///
    /// // Check status
    /// let status = core.proxy().status().await;
    /// println!("Proxy running: {}", status.running);
    ///
    /// // Stop proxy
    /// core.proxy().stop().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn proxy(&self) -> &ProxyService {
        &self.proxy_service
    }

    /// Access the download service for HuggingFace model downloads.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::AppCore;
    /// # async fn example(core: &AppCore) -> anyhow::Result<()> {
    /// // Start a download
    /// core.downloads().download(
    ///     "TheBloke/Llama-2-7B-GGUF".to_string(),
    ///     Some("Q4_K_M".to_string()),
    ///     None,
    /// ).await?;
    ///
    /// // Check active downloads
    /// let active = core.downloads().active_downloads().await;
    /// # Ok(())
    /// # }
    /// ```
    pub fn downloads(&self) -> &DownloadService {
        &self.download_service
    }

    /// Access the settings service for application configuration.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use gglib::services::core::AppCore;
    /// # async fn example(core: &AppCore) -> anyhow::Result<()> {
    /// // Get current settings
    /// let settings = core.settings().get().await?;
    /// println!("Context size: {:?}", settings.default_context_size);
    ///
    /// // Get models directory info
    /// let dir_info = core.settings().get_models_directory_info()?;
    /// println!("Models dir: {}", dir_info.path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn settings(&self) -> &SettingsService {
        &self.settings_service
    }
}

// AppCore is not Clone because ProxyService contains non-Clone RwLock state.
// If cloning is needed, use Arc<AppCore> or access individual services.

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
        let _ = core.servers();
        let _ = core.proxy();
        let _ = core.downloads();
        let _ = core.settings();
    }

    #[tokio::test]
    async fn test_appcore_with_config() {
        let pool = database::setup_database().await.unwrap();
        let core = AppCore::with_config(pool, 8000, 3);
        // Just verify it doesn't panic
        let _ = core.db_pool();
    }
}
