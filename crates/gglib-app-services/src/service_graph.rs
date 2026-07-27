//! One assembly of the domain-ops graph, shared by every GUI adapter.
//!
//! ## Why this module exists
//!
//! The Axum and Tauri adapters each need the same eight `*Ops` wired to the
//! same shared infrastructure, and each had its own copy of that wiring — four
//! copies in total once Tauri's `bootstrap`, `bootstrap_early` and
//! `bootstrap_with` are counted. The copies had already drifted: they
//! disagreed on construction *order*, which matters here, because the shared
//! `ProcessManager` must be built before anything that drives models through
//! it.
//!
//! [`build_service_graph`] is the single assembly. `gglib-app-services` owns
//! every `*Ops` type, so it is where the knowledge of how they fit together
//! belongs.
//!
//! ## What stays with the adapter
//!
//! Only genuinely adapter-shaped concerns: the event emitter (SSE broadcaster
//! vs Tauri handle), server-event sink, HTTP client, semaphores, and each
//! context's own extra fields. `AxumContext` and `TauriContext` keep their
//! existing field names and are populated *from* an [`AppServices`], so no
//! handler call site changes.
//!
//! ## Ordering invariant
//!
//! One `ProcessManager` is created here and shared by `ProxyOps`,
//! `BenchmarkOps` and the public `runtime` handle. That is what enforces
//! "only one llama-server runs at a time system-wide" — constructing a second
//! manager anywhere would silently break it and invite VRAM contention.
//! `BenchmarkOps` layers `CacheRamSetting::ExplicitMb(0)` over the *same*
//! manager rather than owning one, because a prompt cache would perturb its
//! prefill timings.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::events::ServerEvents;
use gglib_core::ports::{
    AppEventEmitter, BenchmarkRepositoryPort, CouncilApprovalRegistryPort, CouncilRepositoryPort,
    DownloadManagerPort, GgufParserPort, HfClientPort, ModelCatalogPort, ModelRepository,
    ModelRuntimePort, ProcessRunner, Repos, SystemProbePort, ToolSupportDetectorPort,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions};
use gglib_core::services::AppCore;
use gglib_mcp::McpService;
use gglib_runtime::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use gglib_runtime::process::ProcessManager;
use gglib_runtime::proxy::ProxySupervisor;

use crate::benchmark::{BenchmarkDeps, BenchmarkOps};
use crate::downloads::{DownloadDeps, DownloadOps};
use crate::mcp::{McpDeps, McpOps};
use crate::models::{ModelDeps, ModelOps};
use crate::proxy::{ProxyDeps, ProxyOps};
use crate::servers::{ServerDeps, ServerOps};
use crate::settings::{SettingsDeps, SettingsOps};
use crate::setup::{SetupDeps, SetupOps};

/// Inputs to [`build_service_graph`].
///
/// Everything the adapter has already built — shared infrastructure from
/// `CoreBootstrap`, plus its own emitter and event sink.
pub struct ServiceGraphParams {
    /// Core application facade.
    pub core: Arc<AppCore>,
    /// Repository container.
    pub repos: Repos,
    /// Process runner from the shared bootstrap.
    pub runner: Arc<dyn ProcessRunner>,
    /// Download manager.
    pub downloads: Arc<dyn DownloadManagerPort>,
    /// HuggingFace client.
    pub hf_client: Arc<dyn HfClientPort>,
    /// GGUF parser.
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Tool-support detector. Supplied by the adapter for the same reason as
    /// `gguf_parser`: the concrete implementation lives in `gglib-gguf`, which
    /// this crate deliberately does not depend on.
    pub tool_detector: Arc<dyn ToolSupportDetectorPort>,
    /// MCP service.
    pub mcp: Arc<McpService>,
    /// Adapter-specific application event emitter.
    pub emitter: Arc<dyn AppEventEmitter>,
    /// Adapter-specific server lifecycle event sink.
    pub server_events: Arc<dyn ServerEvents>,
    /// Council run persistence.
    pub council_repo: Arc<dyn CouncilRepositoryPort>,
    /// HITL approval registry.
    pub approval_registry: Arc<dyn CouncilApprovalRegistryPort>,
    /// Benchmark run persistence.
    pub bench_repo: Arc<dyn BenchmarkRepositoryPort>,
    /// Adapter-supplied base port for llama-server allocation.
    ///
    /// `Some` is an explicit override (a CLI `--base-port`); `None` defers to
    /// `Settings.llama_base_port`, then the compiled default. See
    /// [`resolve_llama_base_port`](crate::proxy::resolve_llama_base_port).
    pub base_port: Option<u16>,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
}

/// The domain-ops graph both GUI adapters share.
///
/// `AxumContext` and `TauriContext` are populated from one of these; they add
/// only their own adapter-specific fields on top.
pub struct AppServices {
    /// Model catalogue operations.
    pub models: Arc<ModelOps>,
    /// Server lifecycle operations.
    pub servers: Arc<ServerOps>,
    /// Download operations.
    pub downloads: Arc<DownloadOps>,
    /// Settings operations.
    pub settings: Arc<SettingsOps>,
    /// MCP operations.
    pub mcp_ops: Arc<McpOps>,
    /// Proxy lifecycle operations.
    pub proxy: Arc<ProxyOps>,
    /// First-run setup operations.
    pub setup: Arc<SetupOps>,
    /// Benchmark operations.
    pub benchmark: Arc<BenchmarkOps>,
    /// Proxy supervisor, shared with `ProxyOps`.
    pub proxy_supervisor: Arc<ProxySupervisor>,
    /// Model repository as a port.
    pub model_repo: Arc<dyn ModelRepository>,
    /// Shared model catalogue.
    pub catalog: Arc<dyn ModelCatalogPort>,
    /// The shared runtime — one llama-server at a time, system-wide.
    pub runtime: Arc<dyn ModelRuntimePort>,
}

/// Build the shared domain-ops graph.
///
/// See the [module docs](self) for the ordering invariant this enforces.
///
/// # Errors
///
/// Returns an error if settings cannot be read or the benchmark HTTP client
/// cannot be constructed.
pub async fn build_service_graph(params: ServiceGraphParams) -> anyhow::Result<AppServices> {
    let ServiceGraphParams {
        core,
        repos,
        runner,
        downloads,
        hf_client,
        gguf_parser,
        mcp,
        emitter,
        server_events,
        tool_detector,
        council_repo,
        approval_registry,
        bench_repo,
        base_port,
        llama_server_path,
    } = params;

    // Resolved here, once, so both adapters honour `Settings.llama_base_port`.
    // Previously only the GUI start path consulted it and the proxy path did
    // not, which is how the two ended up on different ports.
    let settings = core.settings().get().await?;
    let (base_port, base_port_source) = crate::proxy::resolve_llama_base_port(base_port, &settings)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    tracing::debug!(
        port = base_port,
        source = base_port_source,
        "resolved llama base port"
    );

    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(CatalogPortImpl::new(model_repo.clone()));

    // The one manager. Everything that drives a model shares it, which is what
    // makes "one llama-server at a time" an invariant rather than a hope.
    let process_manager = Arc::new(ProcessManager::new_single_swap(
        base_port,
        llama_server_path.to_string_lossy().into_owned(),
        Arc::clone(&catalog),
        ServerConfigOptions::default(),
        // Parity with the CLI proxy: auto-size the host-RAM prompt cache.
        CacheRamSetting::Auto,
    ));
    let runtime: Arc<dyn ModelRuntimePort> =
        Arc::new(RuntimePortImpl::new(Arc::clone(&process_manager)));
    // Same manager, no prompt cache — one would perturb prefill timings and
    // RAM footprint, and benchmarks exist to measure exactly those.
    let benchmark_runtime: Arc<dyn ModelRuntimePort> = Arc::new(RuntimePortImpl::with_cache_ram(
        process_manager,
        CacheRamSetting::ExplicitMb(0),
    ));

    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let system_probe: Arc<dyn SystemProbePort> = Arc::new(gglib_runtime::DefaultSystemProbe::new());

    // ProxyOps is built before ServerOps because ServerOps routes its whole
    // lifecycle through the proxy runtime.
    let proxy = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: Arc::clone(&proxy_supervisor),
        model_repo: model_repo.clone(),
        mcp: Arc::clone(&mcp),
        core: Arc::clone(&core),
        approval_registry,
        council_repo,
        runtime: Arc::clone(&runtime),
    }));

    let models = Arc::new(ModelOps::new(ModelDeps {
        core: Arc::clone(&core),
        runner: Arc::clone(&runner),
        gguf_parser,
    }));

    let servers = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&core),
        proxy: Arc::clone(&proxy),
        emitter,
        server_events,
        tool_detector: Arc::clone(&tool_detector),
    }));

    let download_ops = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: Arc::clone(&downloads),
        hf: Arc::clone(&hf_client),
        tool_detector,
    }));

    let settings = Arc::new(SettingsOps::new(SettingsDeps {
        core: Arc::clone(&core),
        system_probe: Arc::clone(&system_probe),
        downloads: Arc::clone(&downloads),
    }));

    let mcp_ops = Arc::new(McpOps::new(McpDeps {
        mcp: Arc::clone(&mcp),
    }));

    let benchmark = Arc::new(BenchmarkOps::new(BenchmarkDeps {
        model_repo: model_repo.clone(),
        runtime: benchmark_runtime,
        bench_repo,
        http_client: BenchmarkDeps::build_http_client()?,
        settings_repo: repos.settings.clone(),
    }));

    let setup = Arc::new(SetupOps::new(SetupDeps {
        core: Arc::clone(&core),
        system_probe,
    }));

    Ok(AppServices {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        benchmark,
        proxy_supervisor,
        model_repo,
        catalog,
        runtime,
    })
}
