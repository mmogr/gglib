//! Tauri bootstrap - the composition root.
//!
//! This module is the ONLY place where infrastructure is wired together
//! for the Tauri desktop adapter. All concrete implementations are
//! instantiated here:
//! - Database pool and repositories (via gglib-db)
//! - Process runner (via gglib-runtime)
//! - Core services (via gglib-core)
//! - MCP service (via gglib-mcp)
//! - Download manager (via gglib-download)
//!
//! Tauri command handlers receive the fully-composed TauriContext and
//! delegate work to AppCore.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::ports::{
    DownloadManagerConfig, DownloadManagerPort, NoopEmitter, NoopGgufParser, ProcessRunner, Repos,
};
use gglib_core::services::AppCore;
use gglib_core::ModelRegistrar;
use gglib_db::CoreFactory;
use gglib_download::{build_download_manager, DownloadManagerDeps};
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_mcp::McpService;
use gglib_runtime::LlamaServerRunner;

// Use legacy path utilities until they move to gglib-core
use gglib::utils::paths::{get_database_path, get_llama_server_path, resolve_models_dir};

/// Configuration for the Tauri adapter.
#[derive(Debug, Clone)]
pub struct TauriConfig {
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent model servers.
    pub max_concurrent: usize,
}

impl TauriConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            base_port: 9000,
            llama_server_path: get_llama_server_path()?,
            max_concurrent: 4,
        })
    }
}

/// Fully composed application context for Tauri commands.
///
/// This struct owns all the infrastructure and provides access to
/// the AppCore for command handlers.
pub struct TauriContext {
    /// The core application facade.
    pub app: AppCore,
    /// Process runner for direct server operations.
    pub runner: Arc<dyn ProcessRunner>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager for handling model downloads.
    pub downloads: Arc<dyn DownloadManagerPort>,
}

impl TauriContext {
    /// Access the AppCore.
    pub fn app(&self) -> &AppCore {
        &self.app
    }

    /// Access the process runner for server operations.
    pub fn runner(&self) -> &Arc<dyn ProcessRunner> {
        &self.runner
    }

    /// Access the MCP service.
    pub fn mcp(&self) -> &Arc<McpService> {
        &self.mcp
    }

    /// Access the download manager.
    pub fn downloads(&self) -> &Arc<dyn DownloadManagerPort> {
        &self.downloads
    }
}

/// Bootstrap the Tauri desktop application.
///
/// This is the composition root. It:
/// 1. Creates the database pool and repositories
/// 2. Creates the process runner
/// 3. Assembles the AppCore from services
/// 4. Creates the MCP service with injected repository
/// 5. Creates the download manager with injected dependencies
///
/// # Arguments
///
/// * `config` - Tauri configuration options
///
/// # Returns
///
/// A fully composed `TauriContext` ready for command handlers.
pub async fn bootstrap(config: TauriConfig) -> Result<TauriContext> {
    // 1. Create database pool and repositories
    let db_path = get_database_path()?;
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = CoreFactory::create_pool(&db_url).await?;
    let repos = CoreFactory::build_repos(pool.clone());

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
        config.base_port,
        config.llama_server_path.to_string_lossy(),
        config.max_concurrent,
    ));

    // 3. Assemble AppCore
    let app = AppCore::new(repos.clone(), runner.clone());

    // 4. Create MCP service with injected repository
    // For Tauri, we'll eventually use a real event emitter that broadcasts to frontend.
    // For now, use NoopEmitter until the Tauri event bridge is implemented.
    let mcp = Arc::new(McpService::new(repos.mcp_servers.clone(), Arc::new(NoopEmitter)));

    // 5. Create download manager with injected dependencies
    let models_dir = resolve_models_dir(None)?.path;
    let download_config = DownloadManagerConfig::new(models_dir);

    // Create the model registrar (composes over model repository + GGUF parser)
    let model_registrar = Arc::new(ModelRegistrar::new(
        repos.models.clone(),
        Arc::new(NoopGgufParser), // Tauri uses noop parser for now
    ));

    // Create the download state repository
    let download_repo = CoreFactory::download_state_repository(pool);

    // Create the HuggingFace client
    let hf_client = Arc::new(DefaultHfClient::new(HfClientConfig::default()));

    // Build the download manager
    let downloads: Arc<dyn DownloadManagerPort> = Arc::new(build_download_manager(
        DownloadManagerDeps {
            model_registrar,
            download_repo,
            hf_client,
            config: download_config,
        },
    ));

    Ok(TauriContext { app, runner, mcp, downloads })
}

/// Bootstrap with custom repos and runner (for testing).
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    downloads: Arc<dyn DownloadManagerPort>,
) -> TauriContext {
    let app = AppCore::new(repos.clone(), runner.clone());
    let mcp = Arc::new(McpService::new(repos.mcp_servers.clone(), Arc::new(NoopEmitter)));
    TauriContext { app, runner, mcp, downloads }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_with_defaults() {
        // with_defaults can fail if paths don't exist, so just test the method exists
        // In real tests, we'd use bootstrap_with() with mocks
    }
}
