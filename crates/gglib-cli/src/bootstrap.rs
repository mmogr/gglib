//! CLI bootstrap - the composition root.
//!
//! This module is the ONLY place where infrastructure is wired together
//! for the CLI adapter. All concrete implementations are instantiated here:
//! - Database pool and repositories (via gglib-db)
//! - Process runner (via gglib-runtime)
//! - Core services (via gglib-core)
//! - MCP service (via gglib-mcp)
//! - Download manager (via gglib-download)
//!
//! Command handlers receive the fully-composed AppCore and delegate work to it.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::ModelRegistrar;
use gglib_core::ports::{
    DownloadManagerConfig, DownloadManagerPort, GgufParserPort, ModelRepository,
    NoopDownloadEmitter, NoopEmitter, ProcessRunner, Repos,
};
use gglib_core::services::AppCore;
use gglib_db::{CoreFactory, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::GgufParser;
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_mcp::McpService;
use gglib_runtime::LlamaServerRunner;

// Path utilities from core
use gglib_core::paths::{database_path, llama_server_path, resolve_models_dir};

/// Bootstrap configuration for the CLI.
#[derive(Debug, Clone)]
pub struct CliConfig {
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent model servers.
    pub max_concurrent: usize,
}

impl CliConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            base_port: 9000,
            llama_server_path: llama_server_path()?,
            max_concurrent: 4,
        })
    }
}

/// Fully composed application context for CLI commands.
///
/// This struct owns all the infrastructure and provides access to
/// the AppCore for command handlers.
pub struct CliContext {
    /// The core application facade.
    pub app: AppCore,
    /// Process runner for direct server operations.
    pub runner: Arc<dyn ProcessRunner>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager for model downloads.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// GGUF parser for file validation and metadata extraction.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Model repository for proxy catalog access.
    pub model_repo: Arc<dyn ModelRepository>,
    /// Path to llama-server binary.
    pub llama_server_path: PathBuf,
}

impl CliContext {
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

    /// Access the GGUF parser.
    pub fn gguf_parser(&self) -> &Arc<dyn GgufParserPort> {
        &self.gguf_parser
    }

    /// Access the model repository.
    pub fn model_repo(&self) -> &Arc<dyn ModelRepository> {
        &self.model_repo
    }

    /// Access the llama-server path.
    pub fn llama_server_path(&self) -> &PathBuf {
        &self.llama_server_path
    }
}

/// Bootstrap the CLI application.
///
/// This is the composition root. It:
/// 1. Creates the database pool and repositories
/// 2. Creates the process runner
/// 3. Assembles the AppCore from services
/// 4. Creates the MCP service with injected repository
/// 5. Creates the download manager with injected ports
///
/// # Arguments
///
/// * `config` - CLI configuration options
///
/// # Returns
///
/// A fully composed `CliContext` ready for command dispatch.
pub async fn bootstrap(config: CliConfig) -> Result<CliContext> {
    // 1. Create database pool with full schema setup
    let db_path = database_path()?;
    let pool = setup_database(&db_path).await?;
    let repos = CoreFactory::build_repos(pool.clone());

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
        config.llama_server_path.clone(),
        config.max_concurrent,
    ));

    // 3. Create GGUF parser (shared across model registrar and handlers)
    let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(GgufParser::new());

    // 4. Assemble AppCore
    let app = AppCore::new(repos.clone(), runner.clone());

    // 5. Create MCP service with injected repository
    // CLI uses NoopEmitter since there's no frontend to broadcast events to
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));

    // 6. Create download manager with injected ports
    let models_dir_resolution = resolve_models_dir(None)?;
    let download_config = DownloadManagerConfig::new(models_dir_resolution.path);

    // Create the model registrar (composes over model repository + GGUF parser)
    let model_files_repo = Arc::new(gglib_db::repositories::ModelFilesRepository::new(pool.clone()));
    let model_registrar = Arc::new(ModelRegistrar::new(
        repos.models.clone(),
        gguf_parser.clone(), // Share the parser
        Some(model_files_repo as Arc<dyn gglib_core::services::ModelFilesRepositoryPort>),
    ));

    // Create the download state repository
    let download_repo = CoreFactory::download_state_repository(pool);

    // Create the HuggingFace client
    let hf_client = Arc::new(DefaultHfClient::new(&HfClientConfig::default()));

    // Create no-op event emitter (CLI doesn't need real-time events)
    let event_emitter = Arc::new(NoopDownloadEmitter::new());

    // Build the download manager
    let downloads: Arc<dyn DownloadManagerPort> =
        Arc::new(build_download_manager(DownloadManagerDeps {
            model_registrar,
            download_repo,
            hf_client,
            event_emitter,
            config: download_config,
        }));

    Ok(CliContext {
        app,
        runner,
        mcp,
        downloads,
        gguf_parser,
        model_repo: repos.models,
        llama_server_path: config.llama_server_path,
    })
}

/// Bootstrap with custom repos and runner (for testing).
///
/// Note: Uses a stub download manager that does nothing.
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    downloads: Arc<dyn DownloadManagerPort>,
    gguf_parser: Arc<dyn GgufParserPort>,
    llama_server_path: PathBuf,
) -> CliContext {
    let model_repo = repos.models.clone();
    let app = AppCore::new(repos.clone(), runner.clone());
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    CliContext {
        app,
        runner,
        mcp,
        downloads,
        gguf_parser,
        model_repo,
        llama_server_path,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_config_with_defaults() {
        // with_defaults can fail if paths don't exist, so just test the method exists
        // In real tests, we'd use bootstrap_with() with mocks
    }
}
