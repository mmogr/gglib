//! What a proxy run is started with.
//!
//! [`serve`](crate::serve) takes one of these. Every field is public and none
//! has a default, so a caller sets each by name, and a field added later is a
//! compile error wherever one is built in full until it is given a value.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::ProxyAccessConfig;
use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, SettingsRepository};
use gglib_mcp::McpService;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::observers::ProxyObservers;
use crate::slot_eviction::DiskBudget;

/// Everything [`serve`](crate::serve) is given.
pub struct ServeConfig {
    /// The listener to accept on, already bound: the caller binds, so it
    /// knows the address before the server task starts.
    pub listener: TcpListener,
    /// Default context size, for a request that does not name one.
    pub default_ctx: Option<u64>,
    /// Whether this machine's device memory can be read, and therefore
    /// whether a launch with nothing configured gets a context fitted to it.
    /// The caller says, because this crate has no probe and cannot depend on
    /// the crate that does.
    pub device_memory_readable: bool,
    /// Port for managing model runtime.
    pub runtime_port: Arc<dyn ModelRuntimePort>,
    /// Port for listing and resolving models.
    pub catalog_port: Arc<dyn ModelCatalogPort>,
    /// MCP service for the tool gateway.
    pub mcp: Arc<McpService>,
    /// Cancellation token for graceful shutdown: `serve` runs until it fires.
    pub cancel: CancellationToken,
    /// The daemon's own token, when this proxy runs under one, so an
    /// authenticated remote client can stop the whole thing. `None` for an
    /// embedded server: there is no daemon, and the route says so.
    pub daemon_cancel: Option<CancellationToken>,
    /// Settings repository, wrapped in a `SettingsCache` so the per-request
    /// read is served from a short-lived snapshot rather than a query.
    pub settings_repo: Arc<dyn SettingsRepository>,
    /// Operator overrides from this process's command line, applied above the
    /// client's own request parameters. See `SamplingLayers::cli_override`.
    pub inference_override: Option<InferenceConfig>,
    /// Profile applied to a request that names a model without a suffix.
    pub default_profile: Option<String>,
    /// Whether KV cache persistence is enabled.
    pub cache_enabled: bool,
    /// Directory the KV cache slot files are kept in. The eviction sweep runs
    /// over it whenever it is `Some`.
    pub slot_dir: Option<PathBuf>,
    /// Byte budget for the on-disk slot cache eviction sweep. Only consulted
    /// when `slot_dir` is `Some`.
    pub disk_budget: DiskBudget,
    /// Agent-path prompt-cache reuse store, owned by the supervisor so it can
    /// also be shared with the embedded axum server (GUI chat) and outlives a
    /// single proxy run. Exposed on the dashboard as `agent_usage`, alongside
    /// the proxied figure.
    pub agent_metrics: Arc<CacheMetricsStore>,
    /// What this run reports to that outlives it — the per-model defect
    /// counters and the loop guard's log — supervisor-owned for the same
    /// reason as `agent_metrics`.
    pub observers: ProxyObservers,
    /// Who may reach this endpoint: the CORS policy, the optional bearer
    /// token, and the Host allowlist. Carries the `CorsConfig` rather than
    /// sitting beside it: access decisions belong together.
    pub access: ProxyAccessConfig,
}
