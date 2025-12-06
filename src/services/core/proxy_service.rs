//! Proxy service for managing the OpenAI-compatible proxy.
//!
//! This service wraps the proxy module to provide start/stop/status
//! operations for the OpenAI-compatible proxy server.

use super::ModelService;
use crate::services::process_manager::ProcessManager;
use crate::utils::paths::get_llama_server_path;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Status of the proxy server
#[derive(Debug, Clone)]
pub struct ProxyStatus {
    /// Whether the proxy is currently running
    pub running: bool,
    /// Port the proxy is listening on (if running)
    pub port: Option<u16>,
    /// Currently loaded model name (if any)
    pub current_model: Option<String>,
    /// Port of the current llama-server instance (if any)
    pub model_port: Option<u16>,
}

/// Service for managing the OpenAI-compatible proxy server.
///
/// The proxy provides an OpenAI-compatible API that automatically
/// swaps models based on incoming requests.
pub struct ProxyService {
    model_service: Arc<ModelService>,
    proxy_manager: Arc<RwLock<Option<ProcessManager>>>,
    proxy_shutdown: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    proxy_port: Arc<RwLock<Option<u16>>>,
}

impl ProxyService {
    /// Create a new ProxyService.
    ///
    /// # Arguments
    ///
    /// * `model_service` - ModelService for model lookups
    pub fn new(model_service: Arc<ModelService>) -> Self {
        Self {
            model_service,
            proxy_manager: Arc::new(RwLock::new(None)),
            proxy_shutdown: Arc::new(RwLock::new(None)),
            proxy_port: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the OpenAI-compatible proxy server.
    ///
    /// # Arguments
    ///
    /// * `host` - Host to bind to (e.g., "127.0.0.1")
    /// * `port` - Port to listen on
    /// * `start_port` - Base port for llama-server instances
    /// * `default_context` - Default context size when not specified
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Proxy is already running
    /// - Failed to bind to the specified port
    pub async fn start(
        &self,
        host: String,
        port: u16,
        start_port: u16,
        default_context: u64,
    ) -> Result<String> {
        let mut proxy = self.proxy_manager.write().await;
        let mut shutdown = self.proxy_shutdown.write().await;

        if proxy.is_some() {
            return Err(anyhow!("Proxy is already running"));
        }

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Create ProcessManager with SingleSwap strategy for proxy
        let llama_server_path = get_llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string());

        let manager =
            ProcessManager::new_single_swap(Arc::clone(&self.model_service), start_port, llama_server_path);

        // Start proxy server
        crate::proxy::start_proxy_with_shutdown(
            host.clone(),
            port,
            manager.clone(),
            default_context,
            shutdown_rx,
        )
        .await?;

        *proxy = Some(manager);
        *shutdown = Some(shutdown_tx);
        *self.proxy_port.write().await = Some(port);

        info!(host = %host, port = %port, "Proxy started");

        Ok(format!("Proxy started on {}:{}", host, port))
    }

    /// Stop the proxy server.
    ///
    /// # Errors
    ///
    /// Returns an error if the proxy is not running.
    pub async fn stop(&self) -> Result<String> {
        let mut proxy = self.proxy_manager.write().await;
        let mut shutdown = self.proxy_shutdown.write().await;

        if proxy.is_none() {
            return Err(anyhow!("Proxy is not running"));
        }

        // Signal shutdown (this triggers graceful shutdown in the background task)
        if let Some(tx) = shutdown.take() {
            let _ = tx.send(()); // Ignore error if receiver already dropped
        }

        // Clear the manager reference and port
        proxy.take();
        *self.proxy_port.write().await = None;

        info!("Proxy stopped");

        Ok("Proxy stopped".to_string())
    }

    /// Get the current status of the proxy.
    pub async fn status(&self) -> ProxyStatus {
        let proxy = self.proxy_manager.read().await;
        let port = self.proxy_port.read().await;

        if let Some(manager) = proxy.as_ref() {
            let current_model = manager.get_current_model().await;
            let current_port = manager.get_current_port().await;

            ProxyStatus {
                running: true,
                port: *port,
                current_model,
                model_port: current_port,
            }
        } else {
            ProxyStatus {
                running: false,
                port: *port,
                current_model: None,
                model_port: None,
            }
        }
    }

    /// Check if the proxy is currently running.
    pub async fn is_running(&self) -> bool {
        self.proxy_manager.read().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    #[tokio::test]
    async fn test_proxy_service_creation() {
        let pool = database::setup_test_database().await.unwrap();
        let model_service = Arc::new(ModelService::new(pool));
        let service = ProxyService::new(model_service);

        // Should not be running initially
        assert!(!service.is_running().await);
    }

    #[tokio::test]
    async fn test_proxy_status_not_running() {
        let pool = database::setup_test_database().await.unwrap();
        let model_service = Arc::new(ModelService::new(pool));
        let service = ProxyService::new(model_service);

        let status = service.status().await;
        assert!(!status.running);
        assert!(status.port.is_none());
        assert!(status.current_model.is_none());
    }

    #[tokio::test]
    async fn test_proxy_stop_when_not_running() {
        let pool = database::setup_test_database().await.unwrap();
        let model_service = Arc::new(ModelService::new(pool));
        let service = ProxyService::new(model_service);

        // Should error when trying to stop a non-running proxy
        let result = service.stop().await;
        assert!(result.is_err());
    }
}
