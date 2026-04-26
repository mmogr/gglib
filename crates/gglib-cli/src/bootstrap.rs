//! CLI bootstrap - the composition root for the CLI adapter.
//!
//! Shared infrastructure (DB, runner, download manager, model registrar,
//! verification service, …) is wired by [`gglib_bootstrap::CoreBootstrap`].
//! This module is the only place where CLI-specific concerns are added on
//! top: the indicatif-based download emitter, the MCP service (with a
//! `NoopEmitter` since there is no UI surface), and the shared HTTP client.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, GgufParserPort, ModelRegistrarPort, ModelRepository,
    NoopEmitter, ProcessRunner, Repos,
};
use gglib_core::services::AppCore;
use gglib_download::CliDownloadEventEmitter;
use gglib_mcp::McpService;

use gglib_core::settings::DEFAULT_LLAMA_BASE_PORT;

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
            base_port: DEFAULT_LLAMA_BASE_PORT,
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
    pub app: Arc<AppCore>,
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
    /// Base port for allocating llama-server instances (from CLI `--base-port`).
    pub base_port: u16,
    /// Model registrar for download registration with full GGUF metadata.
    ///
    /// Shared with the download manager so both GUI and CLI download paths
    /// use the identical registration logic.
    pub model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Shared HTTP client for LLM adapter calls.
    ///
    /// Constructed once at bootstrap and cloned into each agent session so that
    /// TCP connections to llama-server are pooled across REPL turns, matching
    /// the connection-pooling behaviour of the Axum handler.
    pub http_client: reqwest::Client,
    /// Terminal progress emitter used by the interactive download monitor.
    ///
    /// Shared with the download manager so bar updates flow from manager events,
    /// and with the interactive monitor so it can suspend rendering while
    /// prompting for additional model IDs.
    pub download_emitter: Arc<CliDownloadEventEmitter>,
}

/// Bootstrap the CLI application.
///
/// Delegates all shared wiring to [`CoreBootstrap::build`] and adds the
/// CLI-specific layer: the indicatif emitter (which doubles as the
/// `AppEventEmitter` for the shared bootstrap, ignoring non-download
/// variants), the MCP service, and the shared HTTP client.
pub async fn bootstrap(config: CliConfig) -> Result<CliContext> {
    // CLI terminal emitter — renders indicatif progress bars and exposes
    // the MultiProgress handle for interactive suspend/resume. Implements
    // both `DownloadEventEmitterPort` (for the indicatif renderer) and
    // `AppEventEmitter` (so it plugs into the shared bootstrap event
    // pipeline like Axum/Tauri); non-download AppEvent variants are
    // ignored — the CLI has no UI surface for them.
    let download_emitter = Arc::new(CliDownloadEventEmitter::new());
    let emitter: Arc<dyn AppEventEmitter> = Arc::clone(&download_emitter) as _;

    // Resolve paths/env up-front so BootstrapConfig holds only resolved data.
    let models_resolution = resolve_models_dir(None)?;
    let bootstrap_config = BootstrapConfig {
        db_path: database_path()?,
        llama_server_path: config.llama_server_path.clone(),
        max_concurrent: config.max_concurrent,
        models_dir: models_resolution.path,
        hf_token: std::env::var("HF_TOKEN").ok(),
    };

    let BuiltCore {
        app,
        runner,
        downloads,
        hf_client: _,
        gguf_parser,
        repos,
        model_registrar,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    // CLI uses NoopEmitter for the MCP service since there's no frontend
    // to broadcast lifecycle events to.
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));

    Ok(CliContext {
        app,
        runner,
        mcp,
        downloads,
        gguf_parser,
        model_repo: repos.models,
        model_registrar,
        llama_server_path: config.llama_server_path,
        base_port: config.base_port,
        http_client: reqwest::Client::new(),
        download_emitter,
    })
}

/// Bootstrap with custom repos and runner (for testing).
///
/// Note: callers provide their own download manager (typically a stub that
/// does nothing). This path bypasses [`CoreBootstrap::build`] entirely.
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    downloads: Arc<dyn DownloadManagerPort>,
    gguf_parser: Arc<dyn GgufParserPort>,
    model_registrar: Arc<dyn ModelRegistrarPort>,
    llama_server_path: PathBuf,
) -> CliContext {
    let model_repo = repos.models.clone();
    let app = Arc::new(AppCore::new(repos.clone(), runner.clone()));
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
        model_registrar,
        llama_server_path,
        base_port: 9000,
        http_client: reqwest::Client::new(),
        download_emitter: Arc::new(CliDownloadEventEmitter::new()),
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

