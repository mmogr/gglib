#![doc = include_str!("README.md")]
pub mod models;
pub mod params;
pub mod supervisor;

// Re-export supervisor types
use params::compose_launch_overrides;
pub use params::{PinnedModel, ProxyCacheOptions, StandaloneProxyParams};
pub use supervisor::{ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::council_runner::CouncilRunnerAdapter;
use crate::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use crate::process::ProcessManager;
use gglib_core::domain::council::run::{CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::ports::{
    ApprovalDecision, CouncilApprovalRegistryPort, CouncilRepositoryPort, ModelCatalogPort,
    RepositoryError,
};
use gglib_core::server_config::CacheRamSetting;
use gglib_proxy::CouncilDeps;

// =============================================================================
// Standalone in-memory orchestrator services
// =============================================================================

/// Minimal in-memory approval registry for standalone proxy usage.
///
/// Uses `std::sync::Mutex` so no extra crate dependencies are required.
/// Interactive-mode approval gates work for the lifetime of the proxy process.
struct InMemoryApprovalRegistry {
    pending: StdMutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl InMemoryApprovalRegistry {
    fn new() -> Self {
        Self {
            pending: StdMutex::new(HashMap::new()),
        }
    }
}

impl CouncilApprovalRegistryPort for InMemoryApprovalRegistry {
    fn register(&self, approval_id: String, sender: oneshot::Sender<ApprovalDecision>) {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(approval_id, sender);
    }

    fn resolve(&self, approval_id: &str, decision: ApprovalDecision) -> bool {
        let sender = self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(approval_id);
        if let Some(tx) = sender {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    fn is_pending(&self, approval_id: &str) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(approval_id)
    }
}

/// Minimal in-memory orchestrator repository for standalone proxy usage.
///
/// Stores run records in memory only; no SQLite dep required.
/// Interactive-mode state persists for the lifetime of the proxy process.
struct InMemoryCouncilRepository {
    runs: StdMutex<HashMap<String, CouncilRun>>,
}

impl InMemoryCouncilRepository {
    fn new() -> Self {
        Self {
            runs: StdMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl CouncilRepositoryPort for InMemoryCouncilRepository {
    async fn create_run(&self, run: CouncilRun) -> Result<(), RepositoryError> {
        self.runs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(run.id.clone(), run);
        Ok(())
    }

    async fn update_run_status(
        &self,
        run_id: &str,
        status: CouncilRunStatus,
    ) -> Result<(), RepositoryError> {
        if let Some(run) = self
            .runs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_mut(run_id)
        {
            run.status = status;
        }
        Ok(())
    }

    async fn update_graph(&self, run_id: &str, graph_json: &str) -> Result<(), RepositoryError> {
        if let Some(run) = self
            .runs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get_mut(run_id)
        {
            run.graph_json = Some(graph_json.to_string());
        }
        Ok(())
    }

    async fn append_event(&self, _event: CouncilRunEvent) -> Result<(), RepositoryError> {
        // Event log not needed for standalone proxy.
        Ok(())
    }

    async fn get_run(&self, run_id: &str) -> Result<Option<CouncilRun>, RepositoryError> {
        Ok(self
            .runs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(run_id)
            .cloned())
    }

    async fn list_runs(
        &self,
        status_filter: Option<CouncilRunStatus>,
    ) -> Result<Vec<CouncilRun>, RepositoryError> {
        let guard = self.runs.lock().unwrap_or_else(|p| p.into_inner());
        let runs: Vec<CouncilRun> = guard
            .values()
            .filter(|r| status_filter.as_ref().is_none_or(|s| &r.status == s))
            .cloned()
            .collect();
        Ok(runs)
    }

    async fn list_events(&self, _run_id: &str) -> Result<Vec<CouncilRunEvent>, RepositoryError> {
        Ok(Vec::new())
    }

    async fn truncate_events_after_wave(
        &self,
        _run_id: &str,
        _wave_index: u32,
    ) -> Result<(), RepositoryError> {
        // In-memory repository: no-op.
        Ok(())
    }

    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError> {
        let mut count = 0u64;
        for run in self
            .runs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values_mut()
        {
            if run.status == CouncilRunStatus::Running {
                run.status = CouncilRunStatus::Interrupted;
                count += 1;
            }
        }
        Ok(count)
    }
}

// =============================================================================
// start_proxy_standalone
// =============================================================================

/// Start the OpenAI-compatible proxy as a standalone server (CLI usage).
///
/// The single entry point behind both `gglib proxy` and `gglib serve`. It
/// creates every component internally and blocks until shutdown.
///
/// [`StandaloneProxyParams::pinned`] is what separates the two commands: when
/// set, the process manager refuses every model but the pinned one, giving
/// single-model clients a fixed endpoint. The Axum layer, cache lifecycle,
/// dashboard, SSE, MCP gateway and shutdown path are shared verbatim between
/// the modes — `serve` is a mode of the proxy, not a second stack.
///
/// # Errors
///
/// Returns an error if the HTTP client cannot be built, the proxy cannot bind
/// its address, or shutdown fails.
pub async fn start_proxy_standalone(params: StandaloneProxyParams) -> Result<()> {
    let StandaloneProxyParams {
        host,
        port,
        llama_base_port,
        llama_server_path,
        model_repo,
        mcp,
        settings_repo,
        default_context,
        inference_override,
        cache,
        pinned,
    } = params;

    // Resolve the actual KV cache slot-save directory. `None` whenever the
    // feature is disabled, whatever directory was passed — this is what makes
    // `--cache` off mean zero cache-related flags downstream.
    let slot_save_path = cache.resolved_slot_dir();
    let launch_overrides =
        compose_launch_overrides(&cache, pinned.as_ref(), slot_save_path.clone());

    // Create catalog port from model repository
    let catalog_port: Arc<dyn ModelCatalogPort> =
        Arc::new(CatalogPortImpl::new(Arc::clone(&model_repo)));

    // One manager either way — pinned only changes which models it admits.
    // No explicit cache-RAM value means auto-size rather than "leave the
    // llama-server default": the proxy is the one launch surface where a
    // right-sized prompt cache is the whole point.
    let cache_ram = cache
        .ram_mb
        .map_or(CacheRamSetting::Auto, CacheRamSetting::ExplicitMb);
    let llama_path = llama_server_path.to_string_lossy().into_owned();

    let process_manager = Arc::new(match &pinned {
        Some(model) => ProcessManager::new_pinned(
            model.name.clone(),
            llama_base_port,
            llama_path,
            Arc::clone(&catalog_port),
            launch_overrides,
            cache_ram,
        ),
        None => ProcessManager::new_single_swap(
            llama_base_port,
            llama_path,
            Arc::clone(&catalog_port),
            launch_overrides,
            cache_ram,
        ),
    });

    // Create runtime port
    let runtime_port: Arc<dyn gglib_core::ports::ModelRuntimePort> =
        Arc::new(RuntimePortImpl::new(Arc::clone(&process_manager)));

    // Build CouncilDeps with in-memory backends.
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .build()
        .map_err(|e| anyhow!("failed to build HTTP client: {e}"))?;

    let council_runner = Arc::new(CouncilRunnerAdapter::new(
        Arc::clone(&runtime_port),
        Arc::clone(&catalog_port),
        http_client,
        Arc::clone(&mcp),
    ));
    let orchestrator_deps = CouncilDeps {
        runner: council_runner as Arc<dyn gglib_proxy::CouncilRunnerPort>,
        approval_registry: Arc::new(InMemoryApprovalRegistry::new())
            as Arc<dyn CouncilApprovalRegistryPort>,
        council_repo: Arc::new(InMemoryCouncilRepository::new()) as Arc<dyn CouncilRepositoryPort>,
    };

    // Create supervisor
    let supervisor = ProxySupervisor::new();

    // Start proxy
    let config = ProxyConfig {
        host: host.clone(),
        port,
        default_context,
        cache_enabled: cache.enabled,
        slot_dir: slot_save_path,
        disk_budget: gglib_proxy::slot_eviction::resolve_disk_budget(cache.disk_gb),
        // Passed as its own top-priority sampling layer rather than folded into
        // the persisted global defaults, which sit below the per-model layer.
        inference_override: inference_override.clone(),
    };

    // Initialize MCP service (validates servers and auto-starts enabled ones)
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialization completed with errors: {e}");
    }

    // Gather MCP counts for banner
    let servers = mcp.list_servers().await.unwrap_or_default();
    let eager_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Eager)
        .count();
    let lazy_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Lazy)
        .count();
    let manual_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Manual)
        .count();
    let tools = mcp.list_all_tools().await;
    let tool_count: usize = tools.iter().map(|(_, v)| v.len()).sum();

    // Show startup banner
    println!();
    match pinned.as_ref() {
        Some(_) => println!("  🚀 gglib serve starting (pinned)..."),
        None => println!("  🚀 gglib proxy starting..."),
    }
    println!();
    println!("  Host:            {}", host);
    println!("  Port:            {}", port);
    println!("  Llama base port: {}", llama_base_port);
    println!("  Default context: {}", default_context);
    if let Some(model) = pinned.as_ref() {
        // Stated up front because it changes what the endpoint will accept:
        // every other model is refused rather than swapped in.
        println!(
            "  Pinned model:    {} (id {}) — other models will be refused",
            model.name, model.id
        );
    }
    if let Some(ref ic) = inference_override {
        let mut parts: Vec<String> = Vec::new();
        if let Some(v) = ic.temperature {
            parts.push(format!("temperature={v}"));
        }
        if let Some(v) = ic.top_p {
            parts.push(format!("top_p={v}"));
        }
        if let Some(v) = ic.top_k {
            parts.push(format!("top_k={v}"));
        }
        if let Some(v) = ic.max_tokens {
            parts.push(format!("max_tokens={v}"));
        }
        if let Some(v) = ic.repeat_penalty {
            parts.push(format!("repeat_penalty={v}"));
        }
        if let Some(v) = ic.presence_penalty {
            parts.push(format!("presence_penalty={v}"));
        }
        if let Some(v) = ic.min_p {
            parts.push(format!("min_p={v}"));
        }
        println!("  Inference override: {}", parts.join(", "));
    }
    println!(
        "  MCP servers:     {} (eager: {}, lazy: {}, manual: {})",
        servers.len(),
        eager_count,
        lazy_count,
        manual_count
    );
    println!("  MCP tools:       {} (eager-started)", tool_count);
    println!();

    let addr = supervisor
        .start(
            config,
            runtime_port,
            catalog_port,
            mcp,
            orchestrator_deps,
            settings_repo,
        )
        .await
        .map_err(|e| anyhow!("{e}"))?;
    tracing::info!("Proxy started on {addr}");

    // Show success message with configuration URLs
    println!("  ✓ Proxy started successfully on {}", addr);
    println!();
    println!("  Configure OpenWebUI:");
    println!("    OpenAI API: http://{}/v1", addr);
    println!("    MCP Tools:  http://{}/mcp", addr);
    println!();
    println!("  Press Ctrl+C to stop");
    println!();

    // Wait for Ctrl-C
    tokio::signal::ctrl_c().await?;

    // Show shutdown message
    println!();
    println!("  Shutting down proxy...");

    // Stop proxy
    supervisor.stop().await.map_err(|e| anyhow!("{e}"))?;

    println!("  Proxy stopped");
    println!();

    Ok(())
}
