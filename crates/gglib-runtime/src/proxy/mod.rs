#![doc = include_str!("README.md")]
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
use gglib_core::domain::inference::InferenceConfig;
use gglib_core::ports::{
    ApprovalDecision, CouncilApprovalRegistryPort, CouncilRepositoryPort, ModelCatalogPort,
    ModelRepository, RepositoryError, SettingsRepository,
};
use gglib_core::cache_config::KvCacheType;
use gglib_core::server_config::CacheRamSetting;
use gglib_core::settings::Settings;
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

/// Wraps a `SettingsRepository` and overlays a CLI-supplied `InferenceConfig`
/// on top of whatever the persisted settings contain.
///
/// Fields present in `override_config` win; any field that is `None` there
/// falls back to the persisted global defaults.
struct CliOverrideSettingsRepo {
    inner: Arc<dyn SettingsRepository>,
    override_config: InferenceConfig,
}

#[async_trait]
impl SettingsRepository for CliOverrideSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let mut settings = self.inner.load().await?;
        let merged = self
            .override_config
            .clone()
            .resolve_with_defaults(None, settings.inference_defaults.as_ref());
        settings.inference_defaults = Some(merged);
        Ok(settings)
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        self.inner.save(settings).await
    }
}

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
/// * `inference_override` - Optional once-off inference parameter overrides
///   (applied on top of persisted global defaults; not saved to disk)
/// * `cache_enabled` - Whether to enable KV cache session persistence.
///   `false` means zero behavior change (no `--slot-save-path`/`--cache-ram`
///   flags are ever passed to llama-server).
/// * `slot_dir` - Directory for KV cache slot files. Only consulted when
///   `cache_enabled` is `true`; `None` falls back to `<app-data-dir>/slots`.
/// * `cache_ram_mb` - RAM budget in MiB for llama-server's own host-RAM
///   prompt cache (`--cache-ram`). Independent of `cache_enabled`/`slot_dir`.
///   `None` auto-sizes the budget from system RAM, the model's weights, and
///   its KV footprint at the launch context size (see
///   `gglib_runtime::llama::args::resolve_cache_ram`); pass an explicit value
///   to override, or set `GGLIB_DISABLE_CACHE_AUTOSIZE=1` to fall back to
///   llama-server's own default.
/// * `cache_reuse` - Minimum chunk size in tokens for KV-shift cache reuse
///   past the first prefix divergence (`--cache-reuse`). `None` disables it.
/// * `cache_disk_gb` - Explicit byte budget (in GiB) for the on-disk slot
///   cache eviction sweep (`--cache-disk-gb`). `None` auto-sizes from free
///   disk space at `slot_dir` (see
///   `gglib_proxy::slot_eviction::resolve_disk_budget`), unless
///   `GGLIB_CACHE_DISK_GB` is set.
/// * `cache_type_k` / `cache_type_v` - Explicit overrides for the K/V cache
///   element types (`--cache-type-k`/`--cache-type-v`). `None` resolves to
///   the `q8_0` default per axis, unless `GGLIB_DISABLE_KV_QUANT=1` is set
///   (see `gglib_runtime::llama::args::resolve_kv_cache_types`).
#[allow(clippy::too_many_arguments)]
pub async fn start_proxy_standalone(
    host: String,
    port: u16,
    llama_base_port: u16,
    llama_server_path: PathBuf,
    model_repo: Arc<dyn ModelRepository>,
    default_context: u64,
    mcp: Arc<McpService>,
    settings_repo: Arc<dyn SettingsRepository>,
    inference_override: Option<InferenceConfig>,
    cache_enabled: bool,
    slot_dir: Option<PathBuf>,
    cache_ram_mb: Option<u64>,
    cache_reuse: Option<u32>,
    cache_disk_gb: Option<u64>,
    cache_type_k: Option<KvCacheType>,
    cache_type_v: Option<KvCacheType>,
) -> Result<()> {
    // Resolve the actual KV cache slot-save directory. `None` when the
    // feature is disabled, regardless of what `slot_dir` was passed — this
    // guarantees `--cache` off means zero cache-related flags downstream.
    let slot_save_path: Option<PathBuf> = if cache_enabled {
        Some(slot_dir.unwrap_or_else(|| {
            gglib_core::paths::data_root()
                .map(|d| d.join("slots"))
                .unwrap_or_else(|_| PathBuf::from("slots"))
        }))
    } else {
        None
    };

    // Create catalog port from model repository
    let catalog_port: Arc<dyn ModelCatalogPort> =
        Arc::new(CatalogPortImpl::new(Arc::clone(&model_repo)));

    // Create ProcessManager with SingleSwap strategy for proxy use
    // Now uses resolve_for_launch internally - no path resolver needed
    let process_manager = Arc::new(ProcessManager::new_single_swap(
        llama_base_port,
        llama_server_path.to_string_lossy(),
        Arc::clone(&catalog_port),
        slot_save_path.clone(),
        // No explicit value from the caller means auto-size, not "leave the
        // llama-server default" — the proxy is the one launch surface where a
        // right-sized prompt cache is the whole point.
        cache_ram_mb.map_or(CacheRamSetting::Auto, CacheRamSetting::ExplicitMb),
        cache_reuse,
        cache_type_k,
        cache_type_v,
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

    // Wrap settings_repo with CLI override if any inference flags were supplied
    let effective_settings_repo: Arc<dyn SettingsRepository> =
        if let Some(override_config) = inference_override.clone() {
            Arc::new(CliOverrideSettingsRepo {
                inner: settings_repo,
                override_config,
            })
        } else {
            settings_repo
        };

    // Start proxy
    let config = ProxyConfig {
        host: host.clone(),
        port,
        default_context,
        cache_enabled,
        slot_dir: slot_save_path,
        disk_budget: gglib_proxy::slot_eviction::resolve_disk_budget(cache_disk_gb),
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
            effective_settings_repo,
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
