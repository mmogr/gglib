//! CLI bootstrap - the composition root for the CLI adapter.
//!
//! Shared infrastructure (DB, download manager, model registrar,
//! verification service, …) is wired by [`gglib_bootstrap::CoreBootstrap`].
//! This module is the only place where CLI-specific concerns are added on
//! top: the indicatif-based download emitter, the MCP service, and the
//! shared HTTP client.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, GgufParserPort, ModelCatalogPort, ModelRegistrarPort,
    ModelRepository, SettingsRepository,
};
use gglib_core::services::AppCore;
use gglib_db::SqliteBenchmarkRepository;
use gglib_download::CliDownloadEventEmitter;
use gglib_mcp::McpService;
use gglib_runtime::CatalogPortImpl;

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
}

impl CliConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            base_port: DEFAULT_LLAMA_BASE_PORT,
            llama_server_path: llama_server_path()?,
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
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager for model downloads.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// GGUF parser for file validation and metadata extraction.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Model repository for proxy catalog access.
    pub model_repo: Arc<dyn ModelRepository>,
    /// Shared model catalog, for `gglib_core::request_pipeline::resolve`.
    ///
    /// Commands that compose an agent loop need the target model's
    /// capabilities, tags and inference defaults; resolving them through this
    /// port is what keeps the CLI in step with the proxy.
    pub catalog: Arc<dyn ModelCatalogPort>,
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
    /// Benchmark run repository for compare and perf results.
    pub bench_repo: Arc<SqliteBenchmarkRepository>,
    /// Settings repository for user preferences and inference defaults.
    pub settings_repo: Arc<dyn SettingsRepository>,
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
    // the MultiProgress handle for interactive suspend/resume. It is an
    // `AppEventEmitter` like Axum's and Tauri's, so it plugs straight into
    // the shared bootstrap event pipeline; non-download AppEvent variants
    // are ignored — the CLI has no UI surface for them.
    let download_emitter = Arc::new(CliDownloadEventEmitter::new());
    let emitter: Arc<dyn AppEventEmitter> = Arc::clone(&download_emitter) as _;

    // Resolve paths/env up-front so BootstrapConfig holds only resolved data.
    let models_resolution = resolve_models_dir(None)?;
    let bootstrap_config = BootstrapConfig {
        db_path: database_path()?,
        llama_server_path: config.llama_server_path.clone(),
        models_dir: models_resolution.path,
        hf_token: std::env::var("HF_TOKEN").ok(),
    };

    let BuiltCore {
        app,
        downloads,
        hf_client: _,
        gguf_parser,
        repos,
        model_registrar,
        pool,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    let bench_repo = Arc::new(SqliteBenchmarkRepository::new(pool));

    let mcp = Arc::new(McpService::new(repos.mcp_servers.clone()));

    Ok(CliContext {
        app,
        mcp,
        downloads,
        gguf_parser,
        catalog: Arc::new(CatalogPortImpl::new(Arc::clone(&repos.models))),
        model_repo: repos.models,
        model_registrar,
        llama_server_path: config.llama_server_path,
        base_port: config.base_port,
        http_client: reqwest::Client::new(),
        bench_repo,
        settings_repo: repos.settings,
        download_emitter,
    })
}
