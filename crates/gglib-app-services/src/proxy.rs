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

use gglib_core::ports::{CacheMetricsSink, ModelCatalogPort, ModelRepository, ModelRuntimePort};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::services::AppCore;
use gglib_core::{DEFAULT_LLAMA_BASE_PORT, Settings};
use gglib_mcp::McpService;
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
pub fn resolve_llama_base_port(
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
            runtime: deps.runtime,
        }
    }

    /// Start the proxy server, optionally pinned to a single model.
    ///
    /// Delegates to the supervisor using the shared `ModelRuntimePort`
    /// that was injected at construction time. Returns the bound address.
    ///
    /// A fresh start defines the runtime's pinned mode outright: `Some` is
    /// `gglib serve` (every other model refused), `None` the auto-swapping
    /// proxy. The pin is applied *before* the supervisor starts accepting so
    /// no request can slip a foreign model in between, and it is only
    /// applied on a fresh start — when the proxy is already running the
    /// start fails as a conflict and the standing pin is left untouched.
    ///
    /// # Arguments
    ///
    /// * `config` - Proxy server configuration (host, port, default context)
    /// * `pin` - Pin the runtime to one model for this proxy run
    pub async fn start(
        &self,
        config: ProxyConfig,
        pin: Option<gglib_core::ports::PinnedSpec>,
    ) -> Result<SocketAddr, GuiError> {
        if let ProxyStatus::Running { address } = self.supervisor.status().await {
            return Err(GuiError::Conflict(format!(
                "Proxy already running at {address}"
            )));
        }
        if let Err(e) = self.runtime.set_pin(pin) {
            return Err(GuiError::Internal(format!(
                "could not apply model pin: {e}"
            )));
        }
        self.start_raw(config).await.map_err(Self::map_start_error)
    }

    /// Shared setup behind [`Self::start`] and [`Self::ensure_running`],
    /// returning the untranslated [`SupervisorError`].
    ///
    /// Kept untranslated (rather than eagerly mapped to [`GuiError`]) because
    /// the two callers need to react differently to `AlreadyRunning` versus
    /// `BindFailed` — see [`Self::ensure_running`].
    async fn start_raw(&self, config: ProxyConfig) -> Result<SocketAddr, SupervisorError> {
        // Create catalog port from model repository (cheap wrapper; safe to
        // recreate per call — the underlying model repository is shared).
        let catalog: Arc<dyn ModelCatalogPort> =
            Arc::new(CatalogPortImpl::new(self.model_repo.clone()));

        let runtime = Arc::clone(&self.runtime);

        self.supervisor
            .start(
                config,
                runtime,
                catalog,
                self.mcp.clone(),
                self.core.settings().repo(),
            )
            .await
            // The token the supervisor settled on is deliberately dropped here.
            // These callers are the GUI and the desktop tray, which read it
            // back from settings — the same place the supervisor persisted it —
            // rather than being handed a credential through a start response.
            .map(|bind| bind.addr)
    }

    /// Translate a [`SupervisorError`] into the [`GuiError`] a caller sees.
    ///
    /// `BindFailed` maps to `Conflict`, not `Internal`: from the OS's
    /// perspective the port is simply taken, which is exactly the shape of
    /// error a caller can act on (stop the other process, or use a different
    /// port) — the same category as `AlreadyRunning`, just discovered a layer
    /// lower. It reads as "unexpected" only when nothing else was expected to
    /// be listening there.
    fn map_start_error(e: SupervisorError) -> GuiError {
        match e {
            SupervisorError::AlreadyRunning(addr) => {
                GuiError::Conflict(format!("Proxy already running at {}", addr))
            }
            SupervisorError::BindFailed { address, reason } => GuiError::Conflict(format!(
                "Port {address} is already in use ({reason}) — likely another `gglib serve` \
                 or `gglib proxy` process. Stop it, or change the proxy port in Settings."
            )),
            SupervisorError::NotRunning => {
                GuiError::Internal("Proxy not running (unexpected)".to_string())
            }
            SupervisorError::Internal(msg) => GuiError::Internal(msg),
        }
    }

    /// Start the proxy if it is not already running, and return its address.
    ///
    /// Idempotent, unlike [`Self::start`], which reports an already-running
    /// proxy as a conflict. That distinction matters for callers who need the
    /// proxy up as a precondition rather than as the thing they were asked to
    /// do — starting a model from the GUI, for one.
    ///
    /// Reads the saved `proxy_port`/`default_context_size` settings rather
    /// than `ProxyConfig::default()`'s hardcoded 8080: a user running a
    /// standing `gglib serve`/`gglib proxy` on the default port (the
    /// Copilot-facing case this exists for) would otherwise have every GUI
    /// model start fail with a bind conflict against their own process.
    ///
    /// A concurrent starter racing this same `ProxyOps` is treated as
    /// success: the status check and the start are not atomic, so two
    /// callers can race, and both should end up with a usable address rather
    /// than one seeing a spurious conflict. A bind failure against a
    /// *foreign* process is not recoverable the same way — this supervisor
    /// never started it, so its own status stays `Stopped` — and is
    /// surfaced directly instead.
    ///
    /// # Errors
    ///
    /// Returns `GuiError::Internal` if settings cannot be loaded, and
    /// whatever [`Self::map_start_error`] produces if the proxy cannot start.
    pub async fn ensure_running(&self) -> Result<SocketAddr, GuiError> {
        if let ProxyStatus::Running { address } = self.supervisor.status().await {
            return Ok(address);
        }

        let settings = self
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to load settings: {e}")))?;
        let config = ProxyConfig {
            port: settings.effective_proxy_port(),
            default_context: resolve_context_size(&ServerConfigOptions {
                global_default_ctx: settings.default_context_size,
                ..Default::default()
            }),
            ..ProxyConfig::default()
        };

        match self.start_raw(config).await {
            Ok(address) => Ok(address),
            Err(SupervisorError::AlreadyRunning(_)) => match self.supervisor.status().await {
                ProxyStatus::Running { address } => Ok(address),
                other => Err(GuiError::Internal(format!(
                    "proxy reported as already running but status is {other}"
                ))),
            },
            Err(e) => Err(Self::map_start_error(e)),
        }
    }

    /// The shared model runtime backing this proxy.
    ///
    /// Exposed so other services can drive models through the *same*
    /// `ProcessManager` the proxy uses, preserving the invariant that only one
    /// llama-server runs at a time system-wide.
    #[must_use]
    pub fn runtime(&self) -> &Arc<dyn ModelRuntimePort> {
        &self.runtime
    }

    /// Stop the proxy server.
    ///
    /// Clears any model pin: the pin belongs to one proxy run, and leaving
    /// it set would make the next unpinned start refuse models for no
    /// visible reason.
    pub async fn stop(&self) -> Result<(), GuiError> {
        if let Err(e) = self.runtime.set_pin(None) {
            tracing::debug!("could not clear model pin on stop: {e}");
        }
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

    /// The model the shared runtime is currently pinned to, if any.
    ///
    /// `Some` while a pinned proxy run (`gglib serve`) is active; surfaced in
    /// `GET /api/proxy/status` so clients can tell the two modes apart.
    #[must_use]
    pub fn pinned_model(&self) -> Option<String> {
        self.runtime.pinned_model()
    }

    /// The agent-path prompt-cache reuse sink shared with the proxy dashboard.
    ///
    /// Handlers that run agent loops in this process (GUI chat) record their
    /// reuse here so it lands on the proxy's `agent_usage`.
    /// Always available: the store lives on the supervisor for the process
    /// lifetime, whether or not the proxy is currently running.
    #[must_use]
    pub fn agent_metrics(&self) -> Arc<dyn CacheMetricsSink> {
        self.supervisor.agent_metrics()
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

    // ---------------------------------------------------------------
    // ensure_running — settings-aware port, and BindFailed as Conflict
    // ---------------------------------------------------------------

    /// `ensure_running` must bind the saved `proxy_port`, not the hardcoded
    /// `ProxyConfig::default()` port — otherwise a user with a standing
    /// `gglib serve`/`gglib proxy` on a non-default port would still collide
    /// on 8080 the moment the GUI starts a model.
    #[tokio::test]
    async fn ensure_running_uses_the_saved_proxy_port_setting() {
        let (core, proxy) = crate::test_support::test_core_and_proxy().await;

        let saved_port = 18080;
        core.settings()
            .update(gglib_core::SettingsUpdate {
                proxy_port: Some(Some(saved_port)),
                ..Default::default()
            })
            .await
            .expect("settings update should succeed");

        let addr = proxy
            .ensure_running()
            .await
            .expect("ensure_running should succeed on an unused port");
        assert_eq!(addr.port(), saved_port);

        proxy.stop().await.expect("stop should succeed");
    }

    /// A foreign process already holding the configured port must surface as
    /// a `Conflict` naming the port — not `Internal`, and not the confusing
    /// "reported as already running but status is Stopped" message that
    /// `ensure_running`'s self-race recovery would produce if `BindFailed`
    /// were routed through it: this supervisor never started the foreign
    /// process, so its own status correctly stays `Stopped` throughout.
    #[tokio::test]
    async fn ensure_running_reports_a_clear_conflict_when_the_port_is_taken_by_another_process() {
        let (core, proxy) = crate::test_support::test_core_and_proxy().await;

        // Hold the port ourselves to simulate a foreign gglib process.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind a port for the test");
        let taken_port = listener.local_addr().unwrap().port();

        core.settings()
            .update(gglib_core::SettingsUpdate {
                proxy_port: Some(Some(taken_port)),
                ..Default::default()
            })
            .await
            .expect("settings update should succeed");

        let err = proxy
            .ensure_running()
            .await
            .expect_err("a taken port must not be reported as success");

        let GuiError::Conflict(message) = &err else {
            panic!("expected Conflict, got {err:?}");
        };
        assert!(
            message.contains(&taken_port.to_string()),
            "conflict message should name the port: {message}"
        );

        drop(listener);
    }
}
