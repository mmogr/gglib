//! OpenAI-compatible proxy module.
//!
//! This module provides the proxy supervisor for managing the OpenAI-compatible
//! proxy server lifecycle. The actual HTTP server implementation lives in
//! `gglib-proxy`; this module provides the runtime integration layer.
//!
//! # Architecture
//!
//! - **ProxySupervisor**: Owns proxy state internally, provides start/stop/status
//! - **gglib-proxy**: HTTP server with OpenAI-compatible endpoints
//! - Adapters (Tauri, Axum, CLI) call supervisor methods without storing handles

pub mod models;
pub mod supervisor;

// Re-export supervisor types
pub use supervisor::{ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};

use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::sync::Arc;

use crate::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use crate::process::ProcessManager;
use gglib_core::ports::{ModelCatalogPort, ModelRepository};

/// Start the OpenAI-compatible proxy as a standalone server (CLI usage).
///
/// This is the main entry point for CLI usage. It creates all required
/// components internally and blocks until shutdown.
///
/// # Arguments
///
/// * `host` - Host to bind to (e.g., "127.0.0.1")
/// * `port` - Port to bind to
/// * `llama_base_port` - Base port for llama-server instances
/// * `llama_server_path` - Path to llama-server binary
/// * `model_repo` - Model repository for catalog access
/// * `default_context` - Default context size for models
pub async fn start_proxy_standalone(
    host: String,
    port: u16,
    llama_base_port: u16,
    llama_server_path: PathBuf,
    model_repo: Arc<dyn ModelRepository>,
    default_context: u64,
) -> Result<()> {
    // Create catalog port from model repository
    let catalog_port: Arc<dyn ModelCatalogPort> =
        Arc::new(CatalogPortImpl::new(Arc::clone(&model_repo)));

    // Create ProcessManager with SingleSwap strategy for proxy use
    // Now uses resolve_for_launch internally - no path resolver needed
    let process_manager = Arc::new(ProcessManager::new_single_swap(
        llama_base_port,
        llama_server_path.to_string_lossy(),
        Arc::clone(&catalog_port),
    ));

    // Create runtime port
    let runtime_port: Arc<dyn gglib_core::ports::ModelRuntimePort> =
        Arc::new(RuntimePortImpl::new(Arc::clone(&process_manager)));

    // Create supervisor
    let supervisor = ProxySupervisor::new();

    // Start proxy
    let config = ProxyConfig {
        host: host.clone(),
        port,
        default_context,
    };

    let addr = supervisor
        .start(config, runtime_port, catalog_port)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    tracing::info!("Proxy started on {addr}");

    // Wait for Ctrl-C
    tokio::signal::ctrl_c().await?;

    // Stop proxy
    supervisor.stop().await.map_err(|e| anyhow!("{e}"))?;

    Ok(())
}
