//! Concrete implementation of [`CouncilRunnerPort`] for the proxy.
//!
//! [`CouncilRunnerAdapter`] bridges the proxy's dependency-injection
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
use tracing::debug;

use gglib_agent::council::{CouncilConfig, execute};
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_core::request_pipeline;
use gglib_mcp::McpService;
use gglib_proxy::{CouncilRunParams, CouncilRunnerPort};

use crate::compose::compose_council_ports;

// =============================================================================
// CouncilRunnerAdapter
// =============================================================================

/// Adapts the proxy's [`CouncilRunnerPort`] to the runtime's
/// `gglib-agent` [`execute`] function.
///
/// Constructed once at proxy startup and injected into `CouncilDeps`.
#[derive(Clone)]
pub struct CouncilRunnerAdapter {
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    http_client: Client,
    mcp: Arc<McpService>,
}

impl std::fmt::Debug for CouncilRunnerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CouncilRunnerAdapter").finish()
    }
}

impl CouncilRunnerAdapter {
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
impl CouncilRunnerPort for CouncilRunnerAdapter {
    async fn run(
        &self,
        goal: &str,
        params: CouncilRunParams,
        tx: mpsc::Sender<CouncilEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        // Find the currently loaded model.
        let target = self.runtime_port.current_model().await.ok_or_else(|| {
            anyhow::anyhow!(
                "No model is currently loaded. \
                     Start a model with `gglib model run <name>` or select one in the UI."
            )
        })?;

        debug!(
            model = %target.model_name,
            base_url = %target.base_url,
            goal_prefix = %&goal[..goal.len().min(80)],
            "CouncilRunnerAdapter: starting run"
        );

        // Resolve per-model context (best-effort; passthrough on miss/error).
        let model_context =
            request_pipeline::resolve(self.catalog_port.as_ref(), Some(&target.model_name)).await;

        // Compose infrastructure ports.
        let ports = compose_council_ports(
            target.base_url.clone(),
            self.http_client.clone(),
            None, // use whatever model is loaded
            model_context,
            self.mcp.clone(),
            None, // no sandbox for orchestrator proxy calls
            None, // no sampling override
            None, // agent-path cache sink wired in a later step
        );

        // Build executor config from injected params.
        let config = CouncilConfig {
            hitl_mode: params.hitl_mode,
            approval_registry: params.approval_registry,
            repository: params.council_repo,
            run_id: params.run_id,
            graph_override: params.graph_override,
            ..CouncilConfig::default()
        };

        // Run the orchestrator, aborting on cancel (client disconnect).
        tokio::select! {
            result = execute(goal, &[], ports.llm, ports.tool_executor, config, tx) => {
                result.map_err(|e| anyhow::anyhow!("{e}"))
            }
            _ = cancel.cancelled() => {
                debug!("CouncilRunnerAdapter: run cancelled (client disconnected)");
                Ok(())
            }
        }
    }
}
