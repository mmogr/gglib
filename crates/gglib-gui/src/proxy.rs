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
use gglib_core::{DEFAULT_LLAMA_BASE_PORT, Settings};
use gglib_runtime::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use gglib_runtime::process::ProcessManager;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};
use tracing::info;

use crate::error::GuiError;

/// Resolve the llama-server base port from override, saved settings, or default.
///
/// Precedence: override → settings.llama_base_port → DEFAULT_LLAMA_BASE_PORT
///
/// Validates that the port is in the valid range (1024-65535).
///
/// Returns (port, source_description) for logging.
pub(crate) fn resolve_llama_base_port(
    override_port: Option<u16>,
    settings: &Settings,
) -> Result<(u16, &'static str), GuiError> {
    let (port, source) = if let Some(port) = override_port {
        (port, "override")
    } else if let Some(port) = settings.llama_base_port {
        (port, "saved setting")
    } else {
        (DEFAULT_LLAMA_BASE_PORT, "default")
    };

    // Validate port range
    if !(1024..=65535).contains(&port) {
        return Err(GuiError::Internal(format!(
            "Invalid llama-server base port {}: must be in range 1024-65535",
            port
        )));
    }

    info!(
        port = port,
        source = source,
        "Starting llama-server with base port"
    );

    Ok((port, source))
}

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
    /// * `settings_service` - Settings service for resolving llama-server base port
    /// * `config` - Proxy server configuration (host, port, default context)
    /// * `llama_base_port_override` - Optional override for llama-server base port
    /// * `llama_server_path` - Path to llama-server binary
    pub async fn start(
        &self,
        settings_service: &gglib_core::services::SettingsService,
        config: ProxyConfig,
        llama_base_port_override: Option<u16>,
        llama_server_path: String,
    ) -> Result<SocketAddr, GuiError> {
        // Resolve the llama-server base port from override, settings, or default
        let settings = settings_service
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to load settings: {}", e)))?;
        let (llama_base_port, _source) =
            resolve_llama_base_port(llama_base_port_override, &settings)?;

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
#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::Settings;

    #[test]
    fn test_resolve_llama_base_port_override_wins() {
        let settings = Settings::with_defaults();
        let (port, source) = resolve_llama_base_port(Some(9500), &settings).unwrap();
        assert_eq!(port, 9500);
        assert_eq!(source, "override");
    }

    #[test]
    fn test_resolve_llama_base_port_from_settings() {
        let settings = Settings {
            llama_base_port: Some(9200),
            ..Default::default()
        };
        let (port, source) = resolve_llama_base_port(None, &settings).unwrap();
        assert_eq!(port, 9200);
        assert_eq!(source, "saved setting");
    }

    #[test]
    fn test_resolve_llama_base_port_default_fallback() {
        let settings = Settings::default();
        let (port, source) = resolve_llama_base_port(None, &settings).unwrap();
        assert_eq!(port, DEFAULT_LLAMA_BASE_PORT);
        assert_eq!(source, "default");
    }

    #[test]
    fn test_resolve_llama_base_port_validates_low() {
        let settings = Settings::default();
        let result = resolve_llama_base_port(Some(80), &settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1024-65535"));
    }

    #[test]
    fn test_resolve_llama_base_port_validates_high() {
        let settings = Settings::default();
        // Test that values at the boundary are rejected (65535 is valid, but we can't test higher with u16)
        // So instead test a valid u16 that's outside our port range (ports above 65535 don't fit in u16)
        // Just verify the low boundary works
        assert!(resolve_llama_base_port(Some(65535), &settings).is_ok());
    }

    #[test]
    fn test_resolve_llama_base_port_valid_range() {
        let settings = Settings::default();
        assert!(resolve_llama_base_port(Some(1024), &settings).is_ok());
        assert!(resolve_llama_base_port(Some(65535), &settings).is_ok());
    }
}
