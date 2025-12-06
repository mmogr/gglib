//! Axum server bootstrap - the composition root.
//!
//! This module is the ONLY place where infrastructure is wired together
//! for the Axum web server adapter. All concrete implementations are
//! instantiated here:
//! - Database pool and repositories (via gglib-db)
//! - Process runner (via gglib-runtime)
//! - Core services (via gglib-core)
//!
//! All handler code delegates to AppCore without touching infrastructure.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::ports::{ProcessRunner, Repos};
use gglib_core::services::AppCore;
use gglib_db::CoreFactory;
use gglib_runtime::LlamaServerRunner;

// Import from legacy gglib crate (temporary until handlers migrate here)
use gglib::commands::gui_web::{self, state::AppState};
use gglib::services::gui_backend::GuiBackend;
use gglib::utils::paths::{get_database_path, get_llama_server_path};

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
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            port: 8080,
            base_port: 9000,
            llama_server_path: get_llama_server_path()?,
            max_concurrent: 4,
        })
    }
}

/// Fully composed application context for the Axum server.
pub struct AxumContext {
    /// The core application facade.
    pub app: AppCore,
    /// Process runner for direct server operations.
    pub runner: Arc<dyn ProcessRunner>,
    /// Server configuration.
    pub config: ServerConfig,
}

impl AxumContext {
    /// Access the AppCore.
    pub fn app(&self) -> &AppCore {
        &self.app
    }
}

/// Bootstrap the Axum server application.
///
/// This is the composition root. It:
/// 1. Creates the database pool and repositories
/// 2. Creates the process runner
/// 3. Assembles the AppCore from services
///
/// # Arguments
///
/// * `config` - Server configuration options
///
/// # Returns
///
/// A fully composed `AxumContext` ready for route handling.
pub async fn bootstrap(config: ServerConfig) -> Result<AxumContext> {
    // 1. Create database pool and repositories
    let db_path = get_database_path()?;
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = CoreFactory::create_pool(&db_url).await?;
    let repos = CoreFactory::build_repos(pool);

    // 2. Create process runner
    let runner: Arc<dyn ProcessRunner> = Arc::new(
        LlamaServerRunner::new(
            config.base_port,
            config.llama_server_path.to_string_lossy(),
            config.max_concurrent,
        )
    );

    // 3. Assemble AppCore
    let app = AppCore::new(repos, runner.clone());

    Ok(AxumContext { app, runner, config })
}

/// Bootstrap with custom repos and runner (for testing).
pub fn bootstrap_with(
    repos: Repos,
    runner: Arc<dyn ProcessRunner>,
    config: ServerConfig,
) -> AxumContext {
    let app = AppCore::new(repos, runner.clone());
    AxumContext { app, runner, config }
}

// ============================================================================
// Legacy compatibility layer (to be removed after migration)
// ============================================================================

/// Start the Axum web server (legacy delegation).
///
/// This is the composition root for the web server. It:
/// 1. Creates the GuiBackend (which owns AppCore + ProcessManager)
/// 2. Creates the AppState with the backend
/// 3. Wires up routes to handlers
/// 4. Starts the server
///
/// # Arguments
///
/// * `port` - Port number for the HTTP server
/// * `base_port` - Base port for llama-server instances
/// * `max_concurrent` - Maximum concurrent model servers
///
/// # Returns
///
/// Returns `Result<()>` - server runs until shutdown signal received.
pub async fn start_server(port: u16, base_port: u16, max_concurrent: usize) -> Result<()> {
    // Delegate to legacy implementation
    // In future, this will use the new AppCore directly
    gui_web::start_web_server(port, base_port, max_concurrent).await
}

/// Start the web server with custom GuiBackend (legacy).
pub async fn start_server_with_backend(backend: Arc<GuiBackend>, port: u16) -> Result<()> {
    use std::net::SocketAddr;
    use gglib::download::DownloadEvent;

    tracing::info!("Starting web server on port {}", port);

    // Create application state
    let state = Arc::new(AppState::new(backend.clone()));

    // Wire download events to the broadcast channel
    let progress_tx = state.progress_tx.clone();
    let backend_for_callback = backend.clone();
    backend
        .core()
        .downloads()
        .set_event_callback(Arc::new(move |event: DownloadEvent| {
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = progress_tx.send(json);
            }
            if let DownloadEvent::DownloadCompleted { id, .. } = &event {
                backend_for_callback.core().handle_download_completed(id);
            }
        }));

    // Build router using legacy routes
    let app = gglib::commands::gui_web::routes::api_routes(state);

    // Create socket address
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // Start the server
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Web server listening on http://{}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(backend))
        .await?;

    Ok(())
}

/// Handle graceful shutdown signal.
async fn shutdown_signal(backend: Arc<GuiBackend>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            tracing::info!("Received terminate signal, shutting down...");
        },
    }

    // Stop all running servers
    tracing::info!("Stopping all model servers...");
    if let Err(e) = backend.process_manager().stop_all().await {
        tracing::warn!(error = %e, "Error stopping servers");
    }
}
