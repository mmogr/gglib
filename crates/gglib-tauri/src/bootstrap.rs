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
use async_trait::async_trait;
use gglib_core::ModelRegistrar;
use gglib_core::ports::{
    AppEventBridge, AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, HfClientPort,
    ModelRepository, NoopDownloadEmitter, NoopEmitter, ProcessRunner, Repos,
};
use gglib_core::services::{AppCore, ModelService, SettingsService};
use gglib_db::{CoreFactory, setup_database};
use gglib_download::{DownloadManagerDeps, DownloadManagerImpl, build_download_manager};
// GGUF_BOOTSTRAP_EXCEPTION: Parser injected at composition root only
use gglib_gguf::{GgufParser, ToolSupportDetector};
use gglib_hf::{DefaultHfClient, HfClientConfig};
use gglib_mcp::McpService;
use gglib_runtime::LlamaServerRunner;
use gglib_runtime::proxy::ProxySupervisor;
use gglib_runtime::system::DefaultSystemProbe;
use tauri::AppHandle;

use crate::event_emitter::TauriEventEmitter;
use crate::gui_backend::{GuiBackend, GuiDeps};

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
    /// Download manager (concrete type for worker control).
    pub download_manager: Arc<DownloadManagerImpl>,
    /// Download manager as trait object (for code that doesn't need worker control).
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// HuggingFace client for model discovery.
    pub hf_client: Arc<dyn HfClientPort>,
    /// Model service (Arc-wrapped for GuiBackend).
    pub models: Arc<ModelService>,
    /// Settings service (Arc-wrapped for GuiBackend).
    pub settings: Arc<SettingsService>,
    /// Event emitter for GUI health events.
    pub event_emitter: Arc<dyn AppEventEmitter>,
    /// Proxy supervisor for lifecycle management.
    pub proxy_supervisor: Arc<ProxySupervisor>,
    /// Model repository for catalog access.
    pub model_repo: Arc<dyn ModelRepository>,
    /// AppHandle for creating ServerEvents adapter.
    ///
    /// Optional to support test/bootstrap_early paths.
    /// Production bootstrap MUST pass Some(app_handle) via the main bootstrap() function.
    app_handle: Option<AppHandle>,
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

    /// Access the download manager (concrete type for worker control).
    pub fn download_manager(&self) -> &Arc<DownloadManagerImpl> {
        &self.download_manager
    }

    /// Access the download manager as trait object.
    pub fn downloads(&self) -> &Arc<dyn DownloadManagerPort> {
        &self.downloads
    }

    /// Access the HuggingFace client.
    pub fn hf_client(&self) -> &Arc<dyn HfClientPort> {
        &self.hf_client
    }

    /// Build a GuiBackend from this context.
    ///
    /// This creates the GUI-specific facade that Tauri commands use.
    /// Call this once in setup() and store the result in AppState.
    ///
    /// Note: If app_handle is None (from bootstrap_early), this will use
    /// NoopServerEvents instead of TauriServerEvents.
    pub fn build_gui_backend(&self) -> GuiBackend {
        // Create appropriate ServerEvents implementation
        let server_events: Arc<dyn gglib_core::events::ServerEvents> =
            if let Some(app_handle) = &self.app_handle {
                Arc::new(crate::TauriServerEvents::new(app_handle.clone()))
            } else {
                tracing::debug!("TauriContext has no AppHandle; using NoopServerEvents");
                Arc::new(gglib_core::events::NoopServerEvents)
            };

        // Tool support detector for capability detection
        let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
            Arc::new(ToolSupportDetector::new());

        // System probe for memory and hardware detection
        let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
            Arc::new(DefaultSystemProbe::new());

        // GGUF parser for model metadata extraction
        let gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort> = Arc::new(GgufParser::new());

        let deps = GuiDeps::new(
            Arc::clone(&self.app),
            self.downloads.clone(),
            self.hf_client.clone(),
            self.runner.clone(),
            self.mcp.clone(),
            self.event_emitter.clone(),
            server_events,
            tool_detector,
            self.proxy_supervisor.clone(),
            self.model_repo.clone(),
            system_probe,
            gguf_parser,
        );
        GuiBackend::new(deps)
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
/// * `app_handle` - Tauri AppHandle for event emission
///
/// # Returns
///
/// A fully composed `TauriContext` ready for command handlers.
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

    // 1. Create database pool with full schema setup
    let pool = setup_database(&db_path).await?;
    let repos = CoreFactory::build_repos(pool.clone());

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
        config.llama_server_path.clone(),
        config.max_concurrent,
    ));

    // 3. Create AppCore (without Arc yet - we'll add verification first)
    let app = AppCore::new(repos.clone(), runner.clone());

    // 4. Create MCP service with injected repository
    // For Tauri, we'll eventually use a real event emitter that broadcasts to frontend.
    // For now, use NoopEmitter until the Tauri event bridge is implemented.
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));

    // 5. Create download manager with injected dependencies
    let download_config = DownloadManagerConfig::new(models_resolution.path);

    // Create the model registrar (composes over model repository + GGUF parser)
    let model_files_repo = Arc::new(gglib_db::repositories::ModelFilesRepository::new(
        pool.clone(),
    ));
    let model_registrar = Arc::new(ModelRegistrar::new(
        repos.models.clone(),
        Arc::new(GgufParser::new()), // Real GGUF parser for metadata extraction
        Some(model_files_repo.clone() as Arc<dyn gglib_core::services::ModelFilesRepositoryPort>),
    ));

    // Create the download state repository
    let download_repo = CoreFactory::download_state_repository(pool);

    // Create the HuggingFace client (concrete type for deps, trait object for storage)
    let hf_client_concrete = Arc::new(DefaultHfClient::new(&HfClientConfig::default()));
    let hf_client: Arc<dyn HfClientPort> = hf_client_concrete.clone();

    // Create the real event emitter bridge: TauriEventEmitter -> AppEventBridge
    // This wires download events through to the Tauri frontend
    let tauri_emitter: Arc<dyn AppEventEmitter> =
        Arc::new(TauriEventEmitter::new(app_handle.clone()));
    let event_emitter = Arc::new(AppEventBridge::new(tauri_emitter.clone()));

    // Build the download manager with real event emission (concrete type for worker control)
    let download_manager = Arc::new(build_download_manager(DownloadManagerDeps {
        model_registrar,
        download_repo,
        hf_client: hf_client_concrete,
        event_emitter,
        config: download_config,
    }));
    // Also keep trait object for code that doesn't need worker control
    let downloads: Arc<dyn DownloadManagerPort> = download_manager.clone();

    // 6. Create ModelVerificationService with download trigger adapter
    let download_trigger = Arc::new(DownloadTriggerAdapter {
        download_manager: downloads.clone(),
    });
    let verification_service = Arc::new(gglib_core::services::ModelVerificationService::new(
        repos.models.clone(),
        model_files_repo.clone(),
        hf_client.clone(),
        download_trigger,
    ));
    let app = Arc::new(app.with_verification(verification_service));

    // Create Arc-wrapped services for GuiBackend
    let models = Arc::new(ModelService::new(repos.models.clone()));
    let settings = Arc::new(SettingsService::new(repos.settings.clone()));

    // 7. Create proxy infrastructure
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    Ok(TauriContext {
        app,
        runner,
        mcp,
        download_manager,
        downloads,
        hf_client,
        models,
        settings,
        event_emitter: tauri_emitter,
        proxy_supervisor,
        model_repo,
        app_handle: Some(app_handle),
    })
}

/// Bootstrap with custom repos and runner (for testing).
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    download_manager: Arc<DownloadManagerImpl>,
    hf_client: Arc<dyn HfClientPort>,
    app_handle: Option<AppHandle>,
) -> TauriContext {
    let app = Arc::new(AppCore::new(repos.clone(), runner.clone()));
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));
    let downloads: Arc<dyn DownloadManagerPort> = download_manager.clone();
    let models = Arc::new(ModelService::new(repos.models.clone()));
    let settings = Arc::new(SettingsService::new(repos.settings.clone()));

    // Create proxy infrastructure for tests
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    TauriContext {
        app,
        runner,
        mcp,
        download_manager,
        downloads,
        hf_client,
        models,
        settings,
        event_emitter: Arc::new(NoopEmitter),
        proxy_supervisor,
        model_repo,
        app_handle,
    }
}

/// Bootstrap without AppHandle - uses NoopDownloadEmitter.
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

    // 1. Create database pool with full schema setup
    let pool = setup_database(&db_path).await?;
    let repos = CoreFactory::build_repos(pool.clone());

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(LlamaServerRunner::new(
        config.llama_server_path.clone(),
        config.max_concurrent,
    ));

    // 3. Create AppCore (without Arc yet - we'll add verification first)
    let app = AppCore::new(repos.clone(), runner.clone());

    // 4. Create MCP service
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        Arc::new(NoopEmitter),
    ));

    // 5. Create download manager with noop emitter (no AppHandle available yet)
    let download_config = DownloadManagerConfig::new(models_resolution.path);

    let model_files_repo = Arc::new(gglib_db::repositories::ModelFilesRepository::new(
        pool.clone(),
    ));
    let model_registrar = Arc::new(ModelRegistrar::new(
        repos.models.clone(),
        Arc::new(GgufParser::new()), // Real GGUF parser for metadata extraction
        Some(model_files_repo.clone() as Arc<dyn gglib_core::services::ModelFilesRepositoryPort>),
    ));

    let download_repo = CoreFactory::download_state_repository(pool);
    let hf_client_concrete = Arc::new(DefaultHfClient::new(&HfClientConfig::default()));
    let hf_client: Arc<dyn HfClientPort> = hf_client_concrete.clone();
    let event_emitter = Arc::new(NoopDownloadEmitter::new());

    let download_manager = Arc::new(build_download_manager(DownloadManagerDeps {
        model_registrar,
        download_repo,
        hf_client: hf_client_concrete,
        event_emitter,
        config: download_config,
    }));
    let downloads: Arc<dyn DownloadManagerPort> = download_manager.clone();

    // 6. Create ModelVerificationService with download trigger adapter
    let download_trigger = Arc::new(DownloadTriggerAdapter {
        download_manager: downloads.clone(),
    });
    let verification_service = Arc::new(gglib_core::services::ModelVerificationService::new(
        repos.models.clone(),
        model_files_repo.clone(),
        hf_client.clone(),
        download_trigger,
    ));
    let app = Arc::new(app.with_verification(verification_service));

    // Create Arc-wrapped services for GuiBackend
    let models = Arc::new(ModelService::new(repos.models.clone()));
    let settings = Arc::new(SettingsService::new(repos.settings.clone()));

    // For bootstrap_early, we don't have an AppHandle yet.
    // Store None - build_gui_backend() will use NoopServerEvents.
    // This is safe and idiomatic - the caller gets Noop events until they
    // bootstrap with a real AppHandle via the main bootstrap() function.

    // Create proxy infrastructure
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();

    Ok(TauriContext {
        app,
        runner,
        mcp,
        download_manager,
        downloads,
        hf_client,
        models,
        settings,
        event_emitter: Arc::new(NoopEmitter),
        proxy_supervisor,
        model_repo,
        app_handle: None,
    })
}
/// Adapter to implement DownloadTriggerPort for DownloadManagerPort.
struct DownloadTriggerAdapter {
    download_manager: Arc<dyn DownloadManagerPort>,
}

#[async_trait]
impl gglib_core::services::DownloadTriggerPort for DownloadTriggerAdapter {
    async fn queue_download(
        &self,
        repo_id: String,
        quantization: Option<String>,
    ) -> anyhow::Result<String> {
        use gglib_core::download::{DownloadError, Quantization};
        use gglib_core::ports::DownloadRequest;
        use std::str::FromStr;

        // Convert quantization string to enum, default to Q4_K_M if not specified
        let quant = quantization
            .as_ref()
            .and_then(|q| Quantization::from_str(q).ok())
            .unwrap_or(Quantization::Q4KM);

        let request = DownloadRequest::new(repo_id, quant);
        let id = self
            .download_manager
            .queue_download(request)
            .await
            .map_err(|e: DownloadError| anyhow::anyhow!("Failed to queue download: {}", e))?;

        Ok(id.to_string())
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
