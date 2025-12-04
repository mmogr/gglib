//! Web server startup and configuration.
//!
//! This module handles the initialization and startup of the Axum web server,
//! including static file serving and graceful shutdown.

use crate::commands::gui_web::{routes, state::AppState};
use crate::download::DownloadEvent;
use crate::services::gui_backend::GuiBackend;
use crate::utils::paths::get_resource_root;
use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tracing::warn;

/// Static HTML content embedded in the binary
const INDEX_HTML: &str = include_str!("../../../index.html");

/// Start the web server
///
/// # Arguments
///
/// * `port` - Port number to listen on
/// * `base_port` - Base port for llama-server instances
/// * `max_concurrent` - Maximum concurrent model servers
///
/// # Returns
///
/// Returns `Result<()>` indicating success or failure
pub async fn start_web_server(port: u16, base_port: u16, max_concurrent: usize) -> Result<()> {
    println!("🚀 Starting GGLib Web Server...");

    // Initialize the shared backend
    let backend = Arc::new(GuiBackend::new(base_port, max_concurrent).await?);
    println!("✓ Backend initialized (database + process manager)");

    // Create application state
    let state = Arc::new(AppState::new(backend.clone()));

    // Wire download events to the broadcast channel
    let progress_tx = state.progress_tx.clone();
    backend.core().downloads().set_event_callback(Arc::new(move |event: DownloadEvent| {
        if let Ok(json) = serde_json::to_string(&event) {
            // Ignore send errors (no receivers is fine)
            let _ = progress_tx.send(json);
        }
    }));
    println!("✓ Download events wired to broadcast channel");

    // Build the application router
    let app = build_router(state);

    // Create socket address
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║  GGLib Web Server Running                              ║");
    println!("╠════════════════════════════════════════════════════════╣");
    println!(
        "║  Local:    http://localhost:{}                      ║",
        port
    );
    println!(
        "║  Network:  http://0.0.0.0:{}                        ║",
        port
    );
    println!("╠════════════════════════════════════════════════════════╣");
    println!(
        "║  Model servers will use ports: {}-{}            ║",
        base_port,
        base_port + max_concurrent as u16
    );
    println!("╚════════════════════════════════════════════════════════╝\n");
    println!("Press Ctrl+C to stop the server\n");

    // Start the server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(backend))
        .await?;

    Ok(())
}

/// Build the complete application router
fn build_router(state: Arc<AppState>) -> Router {
    // Determine web_ui path - check multiple locations
    let web_ui_path = find_web_ui_path();

    let serve_dir = ServeDir::new(&web_ui_path);

    Router::new()
        // API routes
        .merge(routes::api_routes(state))
        // Static file serving
        .nest_service("/", serve_dir)
        // Fallback to embedded HTML for production
        .fallback(serve_embedded_html)
}

/// Find the web_ui directory in order of preference:
/// 1. Current directory (for development: `cargo run`)
/// 2. Resource root (for source builds: repo/web_ui)
/// 3. Next to executable (for pre-built binaries)
fn find_web_ui_path() -> std::path::PathBuf {
    // 1. Check current directory first (development)
    let cwd_path = std::path::PathBuf::from("web_ui");
    if cwd_path.exists() && cwd_path.join("index.html").exists() {
        return cwd_path;
    }

    // 2. Check resource root (source builds)
    if let Ok(resource_root) = get_resource_root() {
        let resource_path = resource_root.join("web_ui");
        if resource_path.exists() && resource_path.join("index.html").exists() {
            return resource_path;
        }
    }

    // 3. Check next to executable (pre-built binaries)
    // Note: Cannot use let-chains here as they're unstable on Rust stable
    #[allow(clippy::collapsible_if)]
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_web_ui_path = exe_dir.join("web_ui");
            if exe_web_ui_path.exists() && exe_web_ui_path.join("index.html").exists() {
                return exe_web_ui_path;
            }
        }
    }

    // Fallback to current directory (will use embedded fallback if not found)
    std::path::PathBuf::from("web_ui")
}

/// Serve embedded HTML as fallback
async fn serve_embedded_html() -> axum::response::Html<&'static str> {
    axum::response::Html(INDEX_HTML)
}

/// Handle graceful shutdown signal
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
            println!("\n\n🛑 Shutting down gracefully...");
        },
        _ = terminate => {
            println!("\n\n🛑 Received terminate signal, shutting down...");
        },
    }

    // Stop all running servers
    println!("⏹️  Stopping all model servers...");
    if let Err(e) = backend.process_manager().stop_all().await {
        warn!(error = %e, "Error stopping servers");
    } else {
        println!("✓ All servers stopped");
    }
}
