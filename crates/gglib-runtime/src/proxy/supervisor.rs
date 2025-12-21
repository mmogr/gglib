//! Proxy supervisor for managing the OpenAI-compatible proxy lifecycle.
//!
//! The ProxySupervisor owns the proxy state internally, using tokio::sync::Mutex
//! for async-safe access. Adapters (Tauri, Axum, CLI) call methods on the
//! supervisor without storing handles themselves.
//!
//! Key design decisions:
//! - **Bind-then-report**: TcpListener binds FIRST, then reports real address
//! - **Crash detection**: status() uses cancellation token to distinguish clean stop vs crash
//! - **Internal state ownership**: No distributed state across adapters
//! - **Ports passed to start()**: Allows different port implementations per start

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result as AnyResult;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

/// Handle to a running proxy server.
struct ProxyHandle {
    /// Cancellation token for graceful shutdown.
    cancel_token: CancellationToken,
    /// Join handle for the proxy task (returns Result for error propagation).
    join_handle: JoinHandle<AnyResult<()>>,
    /// Address the proxy is bound to.
    bound_addr: SocketAddr,
}

/// Status of the proxy server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyStatus {
    /// Proxy is not running.
    Stopped,
    /// Proxy is running and listening.
    Running {
        /// Address the proxy is listening on.
        address: SocketAddr,
    },
    /// Proxy started but has crashed/finished unexpectedly (not via cancellation).
    Crashed,
}

impl fmt::Display for ProxyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyStatus::Stopped => write!(f, "Stopped"),
            ProxyStatus::Running { address } => write!(f, "Running on {address}"),
            ProxyStatus::Crashed => write!(f, "Crashed"),
        }
    }
}

/// Error from supervisor operations.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    /// Proxy is already running.
    #[error("Proxy is already running on {0}")]
    AlreadyRunning(SocketAddr),

    /// Failed to bind to address.
    #[error("Failed to bind to {address}: {reason}")]
    BindFailed { address: String, reason: String },

    /// Proxy is not running.
    #[error("Proxy is not running")]
    NotRunning,

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Configuration for starting the proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Host to bind to (e.g., "127.0.0.1" or "0.0.0.0").
    pub host: String,
    /// Port to bind to (0 for auto-assign).
    pub port: u16,
    /// Default context size for models.
    pub default_context: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 11444,
            default_context: 4096,
        }
    }
}

/// Supervisor for managing the OpenAI-compatible proxy.
///
/// Owns the proxy state internally and provides a clean API for
/// starting, stopping, and querying the proxy. Uses tokio::sync::Mutex
/// for async-safe access.
///
/// # Example
///
/// ```ignore
/// let supervisor = ProxySupervisor::new();
/// let addr = supervisor.start(config, runtime_port, catalog_port).await?;
/// println!("Status: {}", supervisor.status().await);
/// supervisor.stop().await?;
/// ```
pub struct ProxySupervisor {
    /// Internal state protected by async mutex.
    handle: Mutex<Option<ProxyHandle>>,
}

impl Default for ProxySupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxySupervisor {
    /// Create a new ProxySupervisor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }

    /// Start the proxy server.
    ///
    /// Binds to the specified address FIRST (bind-then-report pattern),
    /// then spawns the server task with the provided ports.
    ///
    /// # Arguments
    ///
    /// * `config` - Proxy configuration (host, port, default_context)
    /// * `runtime_port` - Port for managing model runtime
    /// * `catalog_port` - Port for listing and resolving models
    ///
    /// # Errors
    ///
    /// Returns error if already running or if bind fails.
    pub async fn start(
        &self,
        config: ProxyConfig,
        runtime_port: Arc<dyn ModelRuntimePort>,
        catalog_port: Arc<dyn ModelCatalogPort>,
    ) -> Result<SocketAddr, SupervisorError> {
        let mut guard = self.handle.lock().await;

        // Check if there's an existing handle
        if let Some(old) = guard.take() {
            if !old.join_handle.is_finished() {
                // Still running - put it back and error
                let addr = old.bound_addr;
                *guard = Some(old);
                return Err(SupervisorError::AlreadyRunning(addr));
            }
            // Finished - log the result if we can get it
            match old.join_handle.await {
                Ok(Ok(())) => debug!("Previous proxy task completed normally"),
                Ok(Err(e)) => warn!("Previous proxy task ended with error: {e}"),
                Err(e) => warn!("Previous proxy task panicked: {e}"),
            }
        }

        // Bind FIRST - get real address before spawning
        let bind_addr = format!("{}:{}", config.host, config.port);
        let listener =
            TcpListener::bind(&bind_addr)
                .await
                .map_err(|e| SupervisorError::BindFailed {
                    address: bind_addr.clone(),
                    reason: e.to_string(),
                })?;

        let bound_addr = listener
            .local_addr()
            .map_err(|e| SupervisorError::Internal(format!("Failed to get local address: {e}")))?;

        info!("Proxy bound to {bound_addr}");

        // Create cancellation token
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();
        let default_ctx = config.default_context;

        // Spawn the proxy task - calls real gglib_proxy::serve
        let join_handle: JoinHandle<AnyResult<()>> = tokio::spawn(async move {
            debug!(
                addr = %bound_addr,
                default_ctx = %default_ctx,
                "Proxy task starting"
            );

            gglib_proxy::serve(
                listener,
                default_ctx,
                runtime_port,
                catalog_port,
                cancel_clone,
            )
            .await
        });

        // Store the handle
        *guard = Some(ProxyHandle {
            cancel_token,
            join_handle,
            bound_addr,
        });

        Ok(bound_addr)
    }

    /// Stop the proxy server.
    ///
    /// Sends cancellation signal and waits for the task to finish.
    /// If the task doesn't stop within 5 seconds, it will be aborted.
    ///
    /// # Errors
    ///
    /// Returns error if not running or if the task panicked/errored.
    pub async fn stop(&self) -> Result<(), SupervisorError> {
        let mut guard = self.handle.lock().await;

        let handle = match guard.take() {
            Some(h) => h,
            None => return Err(SupervisorError::NotRunning),
        };

        info!("Stopping proxy on {}", handle.bound_addr);

        // Signal cancellation
        handle.cancel_token.cancel();

        // Keep ownership of join_handle so we can abort on timeout
        let mut join = handle.join_handle;

        // Wait for task to finish (with timeout)
        match tokio::time::timeout(Duration::from_secs(5), &mut join).await {
            Ok(Ok(Ok(()))) => {
                info!("Proxy stopped cleanly");
                Ok(())
            }
            Ok(Ok(Err(e))) => {
                error!("Proxy task ended with error: {e}");
                Err(SupervisorError::Internal(format!("Proxy error: {e}")))
            }
            Ok(Err(join_err)) => {
                error!("Proxy task panicked: {join_err}");
                Err(SupervisorError::Internal(format!(
                    "Task panicked: {join_err}"
                )))
            }
            Err(_) => {
                // Timed out - abort the task (we still own it)
                warn!("Proxy stop timed out; aborting task");
                join.abort();
                Err(SupervisorError::Internal(
                    "Proxy stop timed out; task aborted".into(),
                ))
            }
        }
    }

    /// Get the current status of the proxy.
    ///
    /// Uses the cancellation token to distinguish between:
    /// - Clean stop (cancelled + finished) -> Stopped
    /// - Crash (not cancelled but finished) -> Crashed
    /// - Running (not finished) -> Running
    pub async fn status(&self) -> ProxyStatus {
        let mut guard = self.handle.lock().await;

        let handle = match guard.as_ref() {
            None => return ProxyStatus::Stopped,
            Some(h) => h,
        };

        if handle.join_handle.is_finished() {
            // Task finished - check if it was cancelled or crashed
            let was_cancelled = handle.cancel_token.is_cancelled();
            *guard = None;

            if was_cancelled {
                // Clean shutdown
                ProxyStatus::Stopped
            } else {
                // Finished without cancellation = crash
                warn!("Detected crashed proxy, cleaning up handle");
                ProxyStatus::Crashed
            }
        } else {
            ProxyStatus::Running {
                address: handle.bound_addr,
            }
        }
    }

    /// Get the bound address if running.
    pub async fn bound_address(&self) -> Option<SocketAddr> {
        let guard = self.handle.lock().await;
        guard.as_ref().and_then(|h| {
            if h.join_handle.is_finished() {
                None
            } else {
                Some(h.bound_addr)
            }
        })
    }
}

impl fmt::Debug for ProxySupervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxySupervisor").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gglib_core::ports::{
        CatalogError, ModelLaunchSpec, ModelRuntimeError, ModelSummary, RunningTarget,
    };

    /// Mock runtime port for testing.
    #[derive(Debug)]
    struct MockRuntimePort;

    #[async_trait]
    impl ModelRuntimePort for MockRuntimePort {
        async fn ensure_model_running(
            &self,
            _model_name: &str,
            _num_ctx: Option<u64>,
            _default_ctx: u64,
        ) -> Result<RunningTarget, ModelRuntimeError> {
            Ok(RunningTarget::local(
                8080,
                1,
                "test-model".to_string(),
                4096,
            ))
        }

        async fn current_model(&self) -> Option<RunningTarget> {
            None
        }

        async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
            Ok(())
        }
    }

    /// Mock catalog port for testing.
    #[derive(Debug)]
    struct MockCatalogPort;

    #[async_trait]
    impl ModelCatalogPort for MockCatalogPort {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            Ok(vec![])
        }

        async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            Ok(None)
        }

        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            Ok(None)
        }
    }

    fn make_ports() -> (Arc<dyn ModelRuntimePort>, Arc<dyn ModelCatalogPort>) {
        (Arc::new(MockRuntimePort), Arc::new(MockCatalogPort))
    }

    #[tokio::test]
    async fn test_supervisor_lifecycle() {
        let supervisor = ProxySupervisor::new();

        // Initially stopped
        assert_eq!(supervisor.status().await, ProxyStatus::Stopped);

        // Start on random port
        let config = ProxyConfig {
            host: "127.0.0.1".to_string(),
            port: 0, // Random port
            default_context: 4096,
        };
        let (runtime, catalog) = make_ports();
        let addr = supervisor
            .start(config.clone(), runtime.clone(), catalog.clone())
            .await
            .unwrap();
        assert_ne!(addr.port(), 0);

        // Should be running
        match supervisor.status().await {
            ProxyStatus::Running { address } => assert_eq!(address, addr),
            other => panic!("Expected Running, got {other:?}"),
        }

        // Can't start again
        let (runtime2, catalog2) = make_ports();
        assert!(matches!(
            supervisor.start(config, runtime2, catalog2).await,
            Err(SupervisorError::AlreadyRunning(_))
        ));

        // Stop
        supervisor.stop().await.unwrap();

        // Should be stopped
        assert_eq!(supervisor.status().await, ProxyStatus::Stopped);

        // Can't stop again
        assert!(matches!(
            supervisor.stop().await,
            Err(SupervisorError::NotRunning)
        ));
    }

    #[tokio::test]
    async fn test_restart_after_stop() {
        let supervisor = ProxySupervisor::new();

        let config = ProxyConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            default_context: 4096,
        };

        // Start
        let (runtime, catalog) = make_ports();
        let addr1 = supervisor
            .start(config.clone(), runtime, catalog)
            .await
            .unwrap();

        // Stop
        supervisor.stop().await.unwrap();

        // Start again (should work)
        let (runtime2, catalog2) = make_ports();
        let addr2 = supervisor.start(config, runtime2, catalog2).await.unwrap();

        // Different port (both were 0 -> random)
        // Note: Could technically get same port, but very unlikely
        assert_ne!(addr1.port(), 0);
        assert_ne!(addr2.port(), 0);

        // Cleanup
        supervisor.stop().await.unwrap();
    }
}
