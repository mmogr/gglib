//! Tauri bootstrap - the composition root for the Tauri desktop adapter.
//!
//! Shared infrastructure (DB, runner, GGUF parser, model registrar/files,
//! HF client, download manager, model verification, AppCore-with-verification)
//! is wired by [`gglib_bootstrap::CoreBootstrap`]. This module adds Tauri-
//! specific concerns on top: the `TauriEventEmitter` (which doubles as the
//! `AppEventEmitter` for the shared bootstrap), the MCP service, the seven
//! domain ops, and the proxy supervisor.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_app_services::{
    AppServices, BenchmarkOps, DownloadOps, McpOps, ModelOps, ProxyOps, ServerOps,
    ServiceGraphParams, SettingsOps, SetupOps, build_service_graph,
};
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, HfClientPort, ModelCatalogPort, ModelRepository,
    ModelRuntimePort, NoopEmitter, Repos,
};
use gglib_core::services::AppCore;
use gglib_db::SqliteBenchmarkRepository;
use gglib_gguf::{GgufParser, ToolSupportDetector};
use gglib_mcp::McpService;
use gglib_runtime::proxy::ProxySupervisor;
use tauri::AppHandle;

use crate::TauriEventEmitter;

// Path utilities from core
use gglib_core::paths::{
    data_root, database_path, llama_server_path, resolve_models_dir, resource_root,
};

/// Configuration for the Tauri adapter.
#[derive(Debug, Clone)]
pub struct TauriConfig {
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent model servers.
    pub max_concurrent: usize,
}

impl TauriConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            llama_server_path: llama_server_path()?,
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
    pub app: Arc<AppCore>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// Download manager.
    ///
    /// Stored as a trait object — no caller in the Tauri adapter depends
    /// on concrete-type methods, so leaking `DownloadManagerImpl` would be
    /// a hexagonal-boundary violation. If a worker-control hook is needed
    /// in future, extend `DownloadManagerPort` rather than re-introducing
    /// the concrete type here.
    pub download_manager: Arc<dyn DownloadManagerPort>,
    /// HuggingFace client for model discovery.
    pub hf_client: Arc<dyn HfClientPort>,
    /// Event emitter for GUI health events.
    pub event_emitter: Arc<dyn AppEventEmitter>,
    /// Proxy supervisor for lifecycle management.
    pub proxy_supervisor: Arc<ProxySupervisor>,
    /// Model repository for catalog access.
    pub model_repo: Arc<dyn ModelRepository>,
    /// Shared model catalog, for `gglib_core::request_pipeline::resolve`.
    ///
    /// Handed to the embedded Axum context so the desktop app's agent and
    /// agent loops resolve per-model context exactly as the CLI and web UI do.
    pub catalog: Arc<dyn ModelCatalogPort>,
    // 7 domain ops
    pub models: Arc<ModelOps>,
    pub servers: Arc<ServerOps>,
    pub downloads: Arc<DownloadOps>,
    pub settings: Arc<SettingsOps>,
    /// Named mcp_ops to avoid clashing with `mcp: Arc<McpService>` above.
    pub mcp_ops: Arc<McpOps>,
    pub proxy: Arc<ProxyOps>,
    pub setup: Arc<SetupOps>,
    /// Benchmark run repository for compare and perf results.
    pub bench_repo: Arc<SqliteBenchmarkRepository>,
    /// Benchmark operations: run_compare and run_perf.
    pub benchmark: Arc<BenchmarkOps>,
    /// Shared `ModelRuntimePort` wrapping the `SingleSwap` `ProcessManager`.
    pub runtime: Arc<dyn ModelRuntimePort>,
}

impl TauriContext {
    /// Access the AppCore.
    pub fn app(&self) -> &Arc<AppCore> {
        &self.app
    }

    /// Access the MCP service.
    pub fn mcp(&self) -> Arc<McpService> {
        Arc::clone(&self.mcp)
    }

    /// Access the download manager.
    pub fn download_manager(&self) -> &Arc<dyn DownloadManagerPort> {
        &self.download_manager
    }

    /// Access the HuggingFace client.
    pub fn hf_client(&self) -> &Arc<dyn HfClientPort> {
        &self.hf_client
    }
}

/// Bootstrap the Tauri desktop application.
pub async fn bootstrap(config: TauriConfig, app_handle: AppHandle) -> Result<TauriContext> {
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(TauriEventEmitter::new(app_handle.clone()));
    let server_events: Arc<dyn gglib_core::events::ServerEvents> =
        Arc::new(crate::TauriServerEvents::new(app_handle));

    bootstrap_inner(config, emitter, server_events, "bootstrap").await
}

/// Bootstrap before Tauri's `setup()` phase, without an `AppHandle`.
///
/// For cases that must bootstrap early (e.g. starting the embedded API
/// server). Download and server events have nowhere to go yet, so both sinks
/// are no-ops; use [`bootstrap`] when an `AppHandle` is available.
///
/// # Errors
///
/// Returns an error if paths cannot be resolved or the core fails to build.
pub async fn bootstrap_early(config: TauriConfig) -> Result<TauriContext> {
    bootstrap_inner(
        config,
        Arc::new(NoopEmitter),
        Arc::new(gglib_core::events::NoopServerEvents),
        "bootstrap_early",
    )
    .await
}

/// The one Tauri bootstrap.
///
/// [`bootstrap`] and [`bootstrap_early`] differ only in where their events go,
/// so they pass different sinks into this and share everything else. The
/// domain-ops graph itself is assembled by
/// [`build_service_graph`](gglib_app_services::build_service_graph), which the
/// Axum adapter also uses.
async fn bootstrap_inner(
    config: TauriConfig,
    emitter: Arc<dyn AppEventEmitter>,
    server_events: Arc<dyn gglib_core::events::ServerEvents>,
    log_label: &str,
) -> Result<TauriContext> {
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
        "Tauri {log_label} resolved paths"
    );

    let bootstrap_config = BootstrapConfig {
        db_path,
        llama_server_path: config.llama_server_path.clone(),
        max_concurrent: config.max_concurrent,
        models_dir: models_resolution.path,
        hf_token: None,
    };
    let BuiltCore {
        app,
        runner: _,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        model_registrar: _,
        pool,
    } = CoreBootstrap::build(bootstrap_config, Arc::clone(&emitter)).await?;

    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    let bench_repo = Arc::new(SqliteBenchmarkRepository::new(pool));

    let AppServices {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        benchmark,
        proxy_supervisor,
        model_repo,
        catalog,
        runtime,
    } = build_service_graph(ServiceGraphParams {
        core: Arc::clone(&app),
        repos: repos.clone(),
        downloads: downloads.clone(),
        hf_client: hf_client.clone(),
        gguf_parser,
        tool_detector: Arc::new(ToolSupportDetector::new()),
        mcp: mcp.clone(),
        emitter: Arc::clone(&emitter),
        server_events,
        bench_repo: Arc::clone(&bench_repo) as Arc<dyn gglib_core::ports::BenchmarkRepositoryPort>,
        // No CLI override on the desktop app, so the saved setting decides.
        base_port: None,
        llama_server_path: config.llama_server_path.clone(),
    })
    .await?;

    Ok(TauriContext {
        app,
        mcp,
        download_manager: downloads,
        hf_client,
        event_emitter: emitter,
        proxy_supervisor,
        model_repo,
        catalog,
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        bench_repo,
        benchmark,
        runtime,
    })
}

/// Bootstrap with custom repos and runner (for testing).
///
/// Shares the same domain-ops graph as the real bootstraps, so a test context
/// cannot silently diverge from production wiring. Only the repositories
/// and event sinks differ.
pub async fn bootstrap_with(
    repos: Repos,
    download_manager: Arc<dyn DownloadManagerPort>,
    hf_client: Arc<dyn HfClientPort>,
    app_handle: Option<AppHandle>,
) -> TauriContext {
    let app = Arc::new(AppCore::new(repos.clone()));
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));

    let server_events: Arc<dyn gglib_core::events::ServerEvents> = match app_handle {
        Some(h) => Arc::new(crate::TauriServerEvents::new(h)),
        None => Arc::new(gglib_core::events::NoopServerEvents),
    };

    let bench_repo = Arc::new(SqliteBenchmarkRepository::new_in_memory_blocking());

    let AppServices {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        benchmark,
        proxy_supervisor,
        model_repo,
        catalog,
        runtime,
    } = build_service_graph(ServiceGraphParams {
        core: Arc::clone(&app),
        repos: repos.clone(),
        downloads: Arc::clone(&download_manager),
        hf_client: Arc::clone(&hf_client),
        gguf_parser: Arc::new(GgufParser::new()),
        tool_detector: Arc::new(ToolSupportDetector::new()),
        mcp: mcp.clone(),
        emitter: Arc::new(NoopEmitter),
        server_events,
        bench_repo: Arc::clone(&bench_repo) as Arc<dyn gglib_core::ports::BenchmarkRepositoryPort>,
        base_port: None,
        llama_server_path: PathBuf::from("llama-server"),
    })
    .await
    .expect("service graph construction should not fail in tests");

    TauriContext {
        app,
        mcp,
        download_manager,
        hf_client,
        event_emitter: Arc::new(NoopEmitter),
        proxy_supervisor,
        model_repo,
        catalog,
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        bench_repo,
        benchmark,
        runtime,
    }
}

// `bootstrap_with` is the only place where the verification service is
// not constructed by `CoreBootstrap`; it deliberately does not attach
// one because the test path supplies its own download manager and does
// not need the file-verification flow.

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_with_defaults() {
        // with_defaults can fail if paths don't exist, so just test the method exists
        // In real tests, we'd use bootstrap_with() with mocks
    }
}
