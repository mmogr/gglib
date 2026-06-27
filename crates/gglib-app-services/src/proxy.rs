//! Proxy operations for GUI backend.
//!
//! Wraps the ProxySupervisor to provide start/stop/status operations
//! for the OpenAI-compatible proxy.
//!
//! The `ModelRuntimePort` (wrapping a shared `SingleSwap` `ProcessManager`) is
//! injected at construction time from the composition root, ensuring that the
//! proxy and any other service that drives llama-server (e.g. benchmarking)
//! share a single manager — preventing VRAM contention.

use std::net::SocketAddr;
use std::sync::Arc;

use gglib_core::ports::{
    CouncilApprovalRegistryPort, CouncilRepositoryPort, ModelCatalogPort, ModelRepository,
    ModelRuntimePort,
};
use gglib_core::services::AppCore;
use gglib_core::{DEFAULT_LLAMA_BASE_PORT, Settings};
use gglib_mcp::McpService;
use gglib_proxy::CouncilDeps;
use gglib_runtime::CouncilRunnerAdapter;
use gglib_runtime::ports_impl::CatalogPortImpl;
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

/// Dependencies for proxy operations.
pub struct ProxyDeps {
    pub supervisor: Arc<ProxySupervisor>,
    pub model_repo: Arc<dyn ModelRepository>,
    pub mcp: Arc<McpService>,
    pub core: Arc<AppCore>,
    /// Shared approval registry for HITL gates (shared with Axum orchestrator handler).
    pub approval_registry: Arc<dyn CouncilApprovalRegistryPort>,
    /// Shared run repository for interactive-mode persistence.
    pub council_repo: Arc<dyn CouncilRepositoryPort>,
    /// Shared runtime port — injected from the composition root so proxy and
    /// benchmark share the same `SingleSwap` `ProcessManager`, enforcing the
    /// invariant that only one llama-server runs at a time system-wide.
    pub runtime: Arc<dyn ModelRuntimePort>,
}

/// Proxy operations facade.
///
/// Provides GUI-friendly interface to proxy lifecycle management.
/// The `ModelRuntimePort` is shared with other services (e.g. benchmarking)
/// via the composition root, so only one llama-server can run system-wide.
pub struct ProxyOps {
    supervisor: Arc<ProxySupervisor>,
    model_repo: Arc<dyn ModelRepository>,
    mcp: Arc<McpService>,
    core: Arc<AppCore>,
    approval_registry: Arc<dyn CouncilApprovalRegistryPort>,
    council_repo: Arc<dyn CouncilRepositoryPort>,
    runtime: Arc<dyn ModelRuntimePort>,
}

impl ProxyOps {
    /// Create proxy operations with the required dependencies.
    pub fn new(deps: ProxyDeps) -> Self {
        Self {
            supervisor: deps.supervisor,
            model_repo: deps.model_repo,
            mcp: deps.mcp,
            core: deps.core,
            approval_registry: deps.approval_registry,
            council_repo: deps.council_repo,
            runtime: deps.runtime,
        }
    }

    /// Start the proxy server.
    ///
    /// Delegates to the supervisor using the shared `ModelRuntimePort`
    /// that was injected at construction time. Returns the bound address.
    ///
    /// # Arguments
    ///
    /// * `config` - Proxy server configuration (host, port, default context)
    pub async fn start(
        &self,
        config: ProxyConfig,
    ) -> Result<SocketAddr, GuiError> {
        // Create catalog port from model repository (cheap wrapper; safe to
        // recreate per call — the underlying model repository is shared).
        let catalog: Arc<dyn ModelCatalogPort> =
            Arc::new(CatalogPortImpl::new(self.model_repo.clone()));

        let runtime = Arc::clone(&self.runtime);

        // Create CouncilDeps — shares approval_registry and council_repo
        // with the main Axum server so interactive-mode runs appear in
        // GET /api/council/runs and can be approved via the Axum API.
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(10)
            .build()
            .map_err(|e| GuiError::Internal(format!("Failed to build HTTP client: {e}")))?;
        let orch_runner = Arc::new(CouncilRunnerAdapter::new(
            Arc::clone(&runtime),
            Arc::clone(&catalog),
            http_client,
            Arc::clone(&self.mcp),
        ));
        let orchestrator = CouncilDeps {
            runner: orch_runner as Arc<dyn gglib_proxy::CouncilRunnerPort>,
            approval_registry: Arc::clone(&self.approval_registry),
            council_repo: Arc::clone(&self.council_repo),
        };

        // Start the proxy
        self.supervisor
            .start(
                config,
                runtime,
                catalog,
                self.mcp.clone(),
                orchestrator,
                self.core.settings().repo(),
            )
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

    /// Get a watch receiver for proxy exit events.
    pub fn exit_receiver(&self) -> tokio::sync::watch::Receiver<ProxyStatus> {
        self.supervisor.exit_receiver()
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
