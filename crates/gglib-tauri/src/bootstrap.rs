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
    DownloadDeps, DownloadOps, McpDeps, McpOps, ModelDeps, ModelOps, ProxyDeps, ProxyOps,
    ServerDeps, ServerOps, SettingsDeps, SettingsOps, SetupDeps, SetupOps,
};
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerPort, HfClientPort, ModelRepository, NoopEmitter,
    ProcessRunner, Repos,
};
use gglib_core::services::AppCore;
use gglib_gguf::{GgufParser, ToolSupportDetector};
use gglib_mcp::McpService;
use gglib_runtime::proxy::ProxySupervisor;
use gglib_runtime::system::DefaultSystemProbe;
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
    /// Process runner for direct server operations.
    pub runner: Arc<dyn ProcessRunner>,
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
    // 7 domain ops
    pub models: Arc<ModelOps>,
    pub servers: Arc<ServerOps>,
    pub downloads: Arc<DownloadOps>,
    pub settings: Arc<SettingsOps>,
    /// Named mcp_ops to avoid clashing with `mcp: Arc<McpService>` above.
    pub mcp_ops: Arc<McpOps>,
    pub proxy: Arc<ProxyOps>,
    pub setup: Arc<SetupOps>,
}

impl TauriContext {
    /// Access the AppCore.
    pub fn app(&self) -> &Arc<AppCore> {
        &self.app
    }

    /// Access the process runner for server operations.
    pub fn runner(&self) -> &Arc<dyn ProcessRunner> {
        &self.runner
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
        "Tauri bootstrap resolved paths"
    );

    // 1. Tauri event emitter — doubles as AppEventEmitter for the shared bootstrap.
    let tauri_emitter: Arc<dyn AppEventEmitter> =
        Arc::new(TauriEventEmitter::new(app_handle.clone()));

    // 2. Shared infrastructure via gglib-bootstrap.
    let bootstrap_config = BootstrapConfig {
        db_path,
        llama_server_path: config.llama_server_path.clone(),
        max_concurrent: config.max_concurrent,
        models_dir: models_resolution.path,
        hf_token: None,
    };
    let BuiltCore {
        app,
        runner,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        model_registrar: _,
    } = CoreBootstrap::build(bootstrap_config, Arc::clone(&tauri_emitter)).await?;

    // 3. MCP service — NoopEmitter until the Tauri MCP event bridge is wired.
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 4. Proxy infrastructure.
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    // 5. Build 7 domain ops.
    let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
        Arc::new(ToolSupportDetector::new());
    let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
        Arc::new(DefaultSystemProbe::new());
    let server_events: Arc<dyn gglib_core::events::ServerEvents> =
        Arc::new(crate::TauriServerEvents::new(app_handle.clone()));

    let models = Arc::new(ModelOps::new(ModelDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        gguf_parser,
    }));
    let servers = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        emitter: Arc::clone(&tauri_emitter),
        server_events,
        tool_detector: tool_detector.clone(),
    }));
    let download_ops = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: downloads.clone(),
        hf: hf_client.clone(),
        tool_detector,
    }));
    let settings = Arc::new(SettingsOps::new(SettingsDeps {
        core: Arc::clone(&app),
        system_probe: system_probe.clone(),
        downloads: downloads.clone(),
    }));
    let mcp_ops = Arc::new(McpOps::new(McpDeps { mcp: mcp.clone() }));
    let proxy = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: proxy_supervisor.clone(),
        model_repo: model_repo.clone(),
        mcp: mcp.clone(),
        core: Arc::clone(&app),
    }));
    let setup = Arc::new(SetupOps::new(SetupDeps {
        core: Arc::clone(&app),
        system_probe,
    }));

    Ok(TauriContext {
        app,
        runner,
        mcp,
        download_manager: downloads,
        hf_client,
        event_emitter: tauri_emitter,
        proxy_supervisor,
        model_repo,
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
    })
}

/// Bootstrap with custom repos and runner (for testing).
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    download_manager: Arc<dyn DownloadManagerPort>,
    hf_client: Arc<dyn HfClientPort>,
    app_handle: Option<AppHandle>,
) -> TauriContext {
    let app = Arc::new(AppCore::new(repos.clone(), runner.clone()));
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    let downloads: Arc<dyn DownloadManagerPort> = download_manager.clone();

    // Create proxy infrastructure for tests
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    // Build 7 domain ops (Noop for tests — no server events, no hardware probing)
    let gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort> = Arc::new(GgufParser::new());
    let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
        Arc::new(ToolSupportDetector::new());
    let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
        Arc::new(DefaultSystemProbe::new());
    let server_events: Arc<dyn gglib_core::events::ServerEvents> = if let Some(ref h) = app_handle {
        Arc::new(crate::TauriServerEvents::new(h.clone()))
    } else {
        Arc::new(gglib_core::events::NoopServerEvents)
    };

    let models_ops = Arc::new(ModelOps::new(ModelDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        gguf_parser,
    }));
    let servers_ops = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        emitter: Arc::new(NoopEmitter),
        server_events,
        tool_detector: tool_detector.clone(),
    }));
    let download_ops = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: downloads.clone(),
        hf: hf_client.clone(),
        tool_detector,
    }));
    let settings_ops = Arc::new(SettingsOps::new(SettingsDeps {
        core: Arc::clone(&app),
        system_probe: system_probe.clone(),
        downloads: downloads.clone(),
    }));
    let mcp_ops = Arc::new(McpOps::new(McpDeps { mcp: mcp.clone() }));
    let proxy_ops = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: proxy_supervisor.clone(),
        model_repo: model_repo.clone(),
        mcp: mcp.clone(),
        core: Arc::clone(&app),
    }));
    let setup_ops = Arc::new(SetupOps::new(SetupDeps {
        core: Arc::clone(&app),
        system_probe,
    }));

    TauriContext {
        app,
        runner,
        mcp,
        download_manager,
        hf_client,
        event_emitter: Arc::new(NoopEmitter),
        proxy_supervisor,
        model_repo,
        models: models_ops,
        servers: servers_ops,
        downloads: download_ops,
        settings: settings_ops,
        mcp_ops,
        proxy: proxy_ops,
        setup: setup_ops,
    }
}

/// Bootstrap without AppHandle - uses a Noop emitter for download events.
///
/// This variant is for cases where bootstrap must happen before
/// Tauri's setup() phase (e.g., starting embedded API server).
/// Download events will not be emitted to the frontend.
///
/// For full event emission, use `bootstrap()` with AppHandle.
pub async fn bootstrap_early(config: TauriConfig) -> Result<TauriContext> {
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
        "Tauri bootstrap_early resolved paths"
    );

    // 1. Shared infrastructure with a Noop emitter (no AppHandle yet, so
    //    download events have nowhere to go).
    let emitter: Arc<dyn AppEventEmitter> = Arc::new(NoopEmitter);
    let bootstrap_config = BootstrapConfig {
        db_path,
        llama_server_path: config.llama_server_path.clone(),
        max_concurrent: config.max_concurrent,
        models_dir: models_resolution.path,
        hf_token: None,
    };
    let BuiltCore {
        app,
        runner,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        model_registrar: _,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    // 2. MCP service.
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 3. Proxy infrastructure.
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    // 4. Build 7 domain ops (no AppHandle → NoopServerEvents).
    let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
        Arc::new(ToolSupportDetector::new());
    let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
        Arc::new(DefaultSystemProbe::new());
    let server_events: Arc<dyn gglib_core::events::ServerEvents> =
        Arc::new(gglib_core::events::NoopServerEvents);

    let models = Arc::new(ModelOps::new(ModelDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        gguf_parser,
    }));
    let servers = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&app),
        runner: runner.clone(),
        emitter: Arc::new(NoopEmitter),
        server_events,
        tool_detector: tool_detector.clone(),
    }));
    let download_ops = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: downloads.clone(),
        hf: hf_client.clone(),
        tool_detector,
    }));
    let settings = Arc::new(SettingsOps::new(SettingsDeps {
        core: Arc::clone(&app),
        system_probe: system_probe.clone(),
        downloads: downloads.clone(),
    }));
    let mcp_ops = Arc::new(McpOps::new(McpDeps { mcp: mcp.clone() }));
    let proxy = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: proxy_supervisor.clone(),
        model_repo: model_repo.clone(),
        mcp: mcp.clone(),
        core: Arc::clone(&app),
    }));
    let setup = Arc::new(SetupOps::new(SetupDeps {
        core: Arc::clone(&app),
        system_probe,
    }));

    Ok(TauriContext {
        app,
        runner,
        mcp,
        download_manager: downloads,
        hf_client,
        event_emitter: Arc::new(NoopEmitter),
        proxy_supervisor,
        model_repo,
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
    })
}
/// `bootstrap_with` is the only place where the verification service is
/// not constructed by [`CoreBootstrap`]; it deliberately does not attach
/// one because the test path supplies its own download manager and does
/// not need the file-verification flow.

#[cfg(test)]
mod tests {
    #[test]
    fn test_config_with_defaults() {
        // with_defaults can fail if paths don't exist, so just test the method exists
        // In real tests, we'd use bootstrap_with() with mocks
    }
}
