//! Axum server bootstrap - the composition root.
//!
//! This module is the ONLY place where infrastructure is wired together
//! for the Axum web adapter. All concrete implementations are instantiated here.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::ModelRegistrar;
use gglib_core::ports::{
    AppEventBridge, DownloadManagerConfig, DownloadManagerPort, HfClientPort, ModelRepository,
    ProcessRunner,
};
use gglib_core::services::AppCore;
use gglib_db::{CoreFactory, setup_database};
use gglib_download::{DownloadManagerDeps, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::{GgufParser, ToolSupportDetector};
use gglib_gui::{GuiBackend, GuiDeps};
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_mcp::McpService;
use gglib_runtime::LlamaServerRunner;
use gglib_runtime::proxy::ProxySupervisor;
use gglib_runtime::system::DefaultSystemProbe;

use crate::sse::SseBroadcaster;

// Path utilities from core
use gglib_core::paths::{
    data_root, database_path, llama_server_path, resolve_models_dir, resource_root,
};

/// CORS configuration for the web server.
#[derive(Debug, Clone, Default)]
pub enum CorsConfig {
    /// Allow all origins (development mode).
    #[default]
    AllowAll,
    /// Allow specific origins (production mode).
    AllowOrigins(Vec<String>),
}

/// Server configuration for the Axum adapter.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Port for the HTTP server.
    pub port: u16,
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent model servers.
    pub max_concurrent: usize,
    /// Optional path to static assets for SPA serving.
    pub static_dir: Option<PathBuf>,
    /// CORS configuration.
    pub cors: CorsConfig,
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            port: 9887,
            base_port: 9000,
            llama_server_path: llama_server_path()?,
            max_concurrent: 4,
            static_dir: None,
            cors: CorsConfig::default(),
        })
    }

    /// Set the static directory for SPA serving.
    #[must_use]
    pub fn with_static_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(path.into());
        self
    }

    /// Set CORS to allow specific origins.
    #[must_use]
    pub fn with_allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.cors = CorsConfig::AllowOrigins(origins);
        self
    }
}

/// Application context for the Axum adapter.
///
/// This struct holds all initialized services for the web server.
/// It mirrors `TauriContext` but is tailored for the Axum web adapter.
pub struct AxumContext {
    /// The GUI backend facade (shared with Tauri).
    pub gui: Arc<GuiBackend>,
    /// The core application facade.
    pub core: Arc<AppCore>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager as trait object.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// HuggingFace client for model discovery.
    pub hf_client: Arc<dyn HfClientPort>,
    /// Process runner for server lifecycle management.
    pub runner: Arc<dyn ProcessRunner>,
    /// SSE broadcaster for real-time events.
    pub sse: Arc<SseBroadcaster>,
}

/// Bootstrap the Axum server with all services.
///
/// This mirrors the Tauri bootstrap pattern exactly, using the same
/// `GuiDeps` → `GuiBackend` construction.
pub async fn bootstrap(config: ServerConfig) -> Result<AxumContext> {
    // Log resolved paths at startup for diagnostics
    let db_path = database_path()?;
    let data_root_path = data_root()?;
    let resource_root_path = resource_root()?;
    let models_resolution = resolve_models_dir(None)?;

    tracing::info!(
        target: "gglib.paths",
        database_path = %db_path.display(),
        data_root = %data_root_path.display(),
        resource_root = %resource_root_path.display(),
        models_dir = %models_resolution.path.display(),
        models_source = ?models_resolution.source,
        llama_server_path = %config.llama_server_path.display(),
        "Axum bootstrap resolved paths"
    );

    // 1. Create database pool with full schema setup
    let pool = setup_database(&db_path).await?;
    let repos = CoreFactory::build_repos(pool.clone());

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
        config.llama_server_path.clone(),
        config.max_concurrent,
    ));

    // 3. Assemble AppCore (as Arc for GuiDeps)
    let core = Arc::new(AppCore::new(repos.clone(), runner.clone()));

    // 4. Bootstrap capabilities for existing models
    if let Err(e) = core.models().bootstrap_capabilities().await {
        tracing::warn!("Failed to bootstrap model capabilities: {}", e);
    }

    // 5. Create SSE broadcaster for real-time events
    let sse = Arc::new(SseBroadcaster::with_defaults());

    // 6. Create MCP service with SSE emitter
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        sse.clone() as Arc<dyn gglib_core::ports::AppEventEmitter>,
    ));

    // 7. Create download manager with SSE emitter
    let download_config = DownloadManagerConfig::new(models_resolution.path);

    let model_registrar = Arc::new(ModelRegistrar::new(
        repos.models.clone(),
        Arc::new(GgufParser::new()),
    ));

    let download_repo = CoreFactory::download_state_repository(pool);
    let hf_client_concrete = Arc::new(DefaultHfClient::new(&HfClientConfig::default()));
    let hf_client: Arc<dyn HfClientPort> = hf_client_concrete.clone();

    // Use AppEventBridge to convert DownloadEvent -> AppEvent -> SSE
    let event_emitter = Arc::new(AppEventBridge::new(sse.clone()));

    let download_manager = Arc::new(build_download_manager(DownloadManagerDeps {
        model_registrar,
        download_repo,
        hf_client: hf_client_concrete,
        event_emitter,
        config: download_config,
    }));
    let downloads: Arc<dyn DownloadManagerPort> = download_manager.clone();

    // 7. Build GuiBackend using shared GuiDeps
    // SSE broadcaster implements AppEventEmitter for health event emission
    // Create server events adapter for lifecycle events
    let server_events = Arc::new(crate::sse::AxumServerEvents::new((*sse).clone()));

    // Tool support detector for capability detection
    let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
        Arc::new(ToolSupportDetector::new());

    // Create proxy infrastructure
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    // System probe for memory and hardware detection
    let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
        Arc::new(DefaultSystemProbe::new());

    // GGUF parser for model metadata extraction
    let gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort> = Arc::new(GgufParser::new());

    let deps = GuiDeps::new(
        Arc::clone(&core),
        downloads.clone(),
        hf_client.clone(),
        runner.clone(),
        mcp.clone(),
        sse.clone() as Arc<dyn gglib_core::ports::AppEventEmitter>,
        server_events as Arc<dyn gglib_core::events::ServerEvents>,
        tool_detector,
        proxy_supervisor,
        model_repo,
        system_probe,
        gguf_parser,
    );
    let gui = Arc::new(GuiBackend::new(deps));

    // Emit initial server snapshot after initialization
    tokio::spawn({
        let gui = Arc::clone(&gui);
        async move {
            gui.emit_initial_snapshot().await;
        }
    });

    Ok(AxumContext {
        gui,
        core,
        mcp,
        downloads,
        hf_client,
        runner,
        sse,
    })
}

/// Start the web server on the specified port.
///
/// If `config.static_dir` is set, serves static assets with SPA fallback.
/// Otherwise, serves only the API endpoints.
pub async fn start_server(config: ServerConfig) -> Result<()> {
    use tokio::net::TcpListener;
    use tracing::info;

    let ctx = bootstrap(config.clone()).await?;

    // Choose router based on whether static serving is configured
    let app = if let Some(ref static_dir) = config.static_dir {
        info!("Serving static assets from: {}", static_dir.display());
        crate::routes::create_spa_router(ctx, static_dir, &config.cors)
    } else {
        crate::routes::create_router(ctx, &config.cors)
    };

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;

    if config.static_dir.is_some() {
        info!("gglib web server (with UI) listening on http://{}", addr);
    } else {
        info!("gglib web server (API only) listening on http://{}", addr);
    }

    axum::serve(listener, app).await?;
    Ok(())
}
