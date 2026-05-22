//! Concrete implementation of [`OrchestratorRunnerPort`] for the proxy.
//!
//! [`OrchestratorRunnerAdapter`] bridges the proxy's dependency-injection
//! boundary: the proxy crate cannot depend on `gglib-runtime` (that would
//! create a circular dependency), so the runtime crate implements the port
//! trait defined in `gglib-proxy` and injects it at startup.
//!
//! # Model selection
//!
//! The adapter calls [`ModelRuntimePort::current_model`] at execution time
//! to find the currently loaded llama-server instance.  If no model is
//! running, the orchestrator returns an error to the caller rather than
//! spinning up a new process (virtual model requests do not carry a `num_ctx`
//! override so there is no clean way to select a context size).

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_core::domain::orchestrator::events::OrchestratorEvent;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_mcp::McpService;
use gglib_proxy::{OrchestratorRunParams, OrchestratorRunnerPort};

use crate::compose::compose_council_ports;

// =============================================================================
// OrchestratorRunnerAdapter
// =============================================================================

/// Adapts the proxy's [`OrchestratorRunnerPort`] to the runtime's
/// `gglib-agent` [`execute`] function.
///
/// Constructed once at proxy startup and injected into `OrchestratorDeps`.
#[derive(Clone)]
pub struct OrchestratorRunnerAdapter {
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    http_client: Client,
    mcp: Arc<McpService>,
}

impl std::fmt::Debug for OrchestratorRunnerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestratorRunnerAdapter").finish()
    }
}

impl OrchestratorRunnerAdapter {
    /// Create a new adapter.
    ///
    /// The supplied ports and client are shared with the rest of the proxy
    /// (reuse the same connection pool and model management state).
    pub fn new(
        runtime_port: Arc<dyn ModelRuntimePort>,
        catalog_port: Arc<dyn ModelCatalogPort>,
        http_client: Client,
        mcp: Arc<McpService>,
    ) -> Self {
        Self {
            runtime_port,
            catalog_port,
            http_client,
            mcp,
        }
    }
}

#[async_trait]
impl OrchestratorRunnerPort for OrchestratorRunnerAdapter {
    async fn run(
        &self,
        goal: &str,
        params: OrchestratorRunParams,
        tx: mpsc::Sender<OrchestratorEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        // Find the currently loaded model.
        let target = self
            .runtime_port
            .current_model()
            .await
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No model is currently loaded. \
                     Start a model with `gglib model run <name>` or select one in the UI."
                )
            })?;

        debug!(
            model = %target.model_name,
            base_url = %target.base_url,
            goal_prefix = %&goal[..goal.len().min(80)],
            "OrchestratorRunnerAdapter: starting run"
        );

        // Fetch model tags for normalisation (best-effort; empty on error).
        let tags = match self.catalog_port.resolve_model(&target.model_name).await {
            Ok(Some(summary)) => summary.tags,
            Ok(None) => {
                warn!(model = %target.model_name, "orchestrator runner: model not in catalog; using empty tags");
                Vec::new()
            }
            Err(e) => {
                warn!(model = %target.model_name, error = %e, "orchestrator runner: failed to resolve model tags");
                Vec::new()
            }
        };

        // Compose infrastructure ports.
        let ports = compose_council_ports(
            target.base_url.clone(),
            self.http_client.clone(),
            None, // use whatever model is loaded
            tags,
            self.mcp.clone(),
            None, // no sandbox for orchestrator proxy calls
        );

        // Build executor config from injected params.
        let config = OrchestratorConfig {
            hitl_mode: params.hitl_mode,
            approval_registry: params.approval_registry,
            repository: params.orchestrator_repo,
            run_id: params.run_id,
            graph_override: params.graph_override,
            ..OrchestratorConfig::default()
        };

        // Run the orchestrator, aborting on cancel (client disconnect).
        tokio::select! {
            result = execute(goal, &[], ports.llm, ports.tool_executor, config, tx) => {
                result.map_err(|e| anyhow::anyhow!("{e}"))
            }
            _ = cancel.cancelled() => {
                debug!("OrchestratorRunnerAdapter: run cancelled (client disconnected)");
                Ok(())
            }
        }
    }
}
