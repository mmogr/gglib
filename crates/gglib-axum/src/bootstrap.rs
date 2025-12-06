//! Axum server bootstrap - the composition root.
//!
//! This module provides the server startup logic as a composition root.
//! It is the ONLY place where infrastructure is wired together:
//! - GuiBackend creation
//! - AppState construction  
//! - Route wiring
//! - Server startup
//!
//! All handler code delegates to GuiBackend without touching infrastructure.

use anyhow::Result;
use std::sync::Arc;

// Import from legacy gglib crate (temporary until handlers migrate here)
use gglib::commands::gui_web::{self, state::AppState};
use gglib::services::gui_backend::GuiBackend;

/// Start the Axum web server.
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
    // In future, this will be the actual implementation
    gui_web::start_web_server(port, base_port, max_concurrent).await
}

/// Start the web server with custom GuiBackend.
///
/// This variant allows injecting a pre-configured backend,
/// useful for testing or when the backend is already created.
///
/// # Arguments
///
/// * `backend` - Pre-configured GuiBackend
/// * `port` - Port number for the HTTP server
///
/// # Returns
///
/// Returns `Result<()>` - server runs until shutdown signal received.
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
