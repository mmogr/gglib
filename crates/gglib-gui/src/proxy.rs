//! Proxy operations for GUI backend.
//!
//! Wraps the ProxySupervisor to provide start/stop/status operations
//! for the OpenAI-compatible proxy.
//!
//! The SingleSwap ProcessManager is created on-demand when starting
//! the proxy, ensuring it's independent from the GUI's Concurrent
//! process management.

use std::net::SocketAddr;
use std::sync::Arc;

use gglib_core::ports::{ModelCatalogPort, ModelRepository, ModelRuntimePort};
use gglib_runtime::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use gglib_runtime::process::ProcessManager;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};

use crate::error::GuiError;

/// Proxy operations facade.
///
/// Provides GUI-friendly interface to proxy lifecycle management.
/// Creates port implementations and ProcessManager on-demand when starting.
pub struct ProxyOps {
    supervisor: Arc<ProxySupervisor>,
    model_repo: Arc<dyn ModelRepository>,
}

impl ProxyOps {
    /// Create proxy operations with the required dependencies.
    pub fn new(supervisor: Arc<ProxySupervisor>, model_repo: Arc<dyn ModelRepository>) -> Self {
        Self {
            supervisor,
            model_repo,
        }
    }

    /// Start the proxy server.
    ///
    /// Creates a SingleSwap ProcessManager and port implementations,
    /// then delegates to the supervisor. Returns the bound address.
    ///
    /// # Arguments
    ///
    /// * `config` - Proxy server configuration (host, port, default context)
    /// * `llama_base_port` - Base port for llama-server instances
    /// * `llama_server_path` - Path to llama-server binary
    pub async fn start(
        &self,
        config: ProxyConfig,
        llama_base_port: u16,
        llama_server_path: String,
    ) -> Result<SocketAddr, GuiError> {
        // Create catalog port from model repository
        let catalog: Arc<dyn ModelCatalogPort> =
            Arc::new(CatalogPortImpl::new(self.model_repo.clone()));

        // Create a fresh SingleSwap ProcessManager for this proxy session
        let process_manager = Arc::new(ProcessManager::new_single_swap(
            llama_base_port,
            llama_server_path,
            catalog.clone(),
        ));

        // Create runtime port wrapping the process manager
        let runtime: Arc<dyn ModelRuntimePort> = Arc::new(RuntimePortImpl::new(process_manager));

        // Start the proxy
        self.supervisor
            .start(config, runtime, catalog)
            .await
            .map_err(|e| match e {
                SupervisorError::AlreadyRunning(addr) => {
                    GuiError::Conflict(format!("Proxy already running at {}", addr))
                }
                SupervisorError::BindFailed { address, reason } => {
                    GuiError::Internal(format!("Failed to bind proxy to {}: {}", address, reason))
                }
                SupervisorError::NotRunning => {
                    GuiError::Internal("Proxy not running (unexpected)".to_string())
                }
                SupervisorError::Internal(msg) => GuiError::Internal(msg),
            })
    }

    /// Stop the proxy server.
    pub async fn stop(&self) -> Result<(), GuiError> {
        self.supervisor.stop().await.map_err(|e| match e {
            SupervisorError::NotRunning => GuiError::Conflict("Proxy is not running".to_string()),
            SupervisorError::AlreadyRunning(addr) => {
                GuiError::Internal(format!("Proxy unexpectedly running at {}", addr))
            }
            SupervisorError::BindFailed { address, reason } => {
                GuiError::Internal(format!("Unexpected bind error at {}: {}", address, reason))
            }
            SupervisorError::Internal(msg) => GuiError::Internal(msg),
        })
    }

    /// Get the current proxy status.
    pub async fn status(&self) -> ProxyStatus {
        self.supervisor.status().await
    }
}
