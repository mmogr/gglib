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
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use crate::council_runner::CouncilRunnerAdapter;
use crate::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use crate::process::ProcessManager;
use gglib_core::domain::council::run::{CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::ports::{
    ApprovalDecision, CouncilApprovalRegistryPort, CouncilRepositoryPort, ModelCatalogPort,
    ModelRepository, RepositoryError, SettingsRepository,
};
use gglib_mcp::McpService;
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
/// * `mcp` - MCP service for tool gateway
/// * `settings_repo` - Settings repository for global inference defaults
pub async fn start_proxy_standalone(
    host: String,
    port: u16,
    llama_base_port: u16,
    llama_server_path: PathBuf,
    model_repo: Arc<dyn ModelRepository>,
    default_context: u64,
    mcp: Arc<McpService>,
    settings_repo: Arc<dyn SettingsRepository>,
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
    println!("  🚀 gglib proxy starting...");
    println!();
    println!("  Host:            {}", host);
    println!("  Port:            {}", port);
    println!("  Llama base port: {}", llama_base_port);
    println!("  Default context: {}", default_context);
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
