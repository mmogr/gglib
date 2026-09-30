//! What a proxy is started with.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::DevicePorts;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::RemoteGatewayPort;
use gglib_core::settings::DEFAULT_PROXY_PORT;
use gglib_proxy::slot_eviction::DiskBudget;

/// Configuration for starting the proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Host to bind to (e.g., "127.0.0.1" or "0.0.0.0").
    pub host: String,
    /// Port to bind to (0 for auto-assign).
    pub port: u16,
    /// The user's global default context, or `None` when they never set one.
    ///
    /// `Option`, not a `u64` pre-filled with the floor: pre-resolving it here
    /// meant every launch arrived with `global_default_ctx = Some(4096)`
    /// whether or not anyone had chosen 4096, which made every rung below it
    /// — including the fitted one — unreachable dead code.
    pub default_context: Option<u64>,
    /// Whether KV cache session persistence is enabled for this proxy run.
    /// `false` (the default) means zero behavior change — no
    /// `--slot-save-path`/`--cache-ram` flags are ever passed to llama-server.
    pub cache_enabled: bool,
    /// Directory for KV cache slot files. Only consulted when `cache_enabled`
    /// is `true`; `None` falls back to `<app-data-dir>/slots`.
    pub slot_dir: Option<PathBuf>,
    /// Byte budget for the on-disk slot cache eviction sweep. Only consulted
    /// when `cache_enabled` is `true`.
    pub disk_budget: DiskBudget,
    /// Operator overrides from the command line (`gglib proxy`/`serve
    /// --temperature …`), applied above the client's own request
    /// parameters. `None` means the client and the stored layers decide.
    pub inference_override: Option<InferenceConfig>,
    /// Profile applied to requests naming the model without a suffix.
    pub default_profile: Option<String>,
    /// Bearer token demanded on `/v1/*` and `/mcp`, as supplied by `--api-key`
    /// or `GGLIB_API_KEY`. `None` falls through to the stored setting, and then
    /// to generating one when the bind is not loopback — see
    /// [`ProxySupervisor::start`](super::ProxySupervisor::start).
    pub api_key: Option<String>,
    /// The daemon's cancellation token, when this proxy is being started by
    /// one. Passed through to the remote shutdown route, which is the only
    /// thing that uses it; `None` for an embedded server, where there is no
    /// daemon to stop.
    pub daemon_cancel: Option<tokio_util::sync::CancellationToken>,
    /// Host header values to accept beyond loopback and the bound address
    /// (`--allowed-host`). Empty is the norm; a wildcard bind is the case that
    /// needs it.
    pub allowed_hosts: Vec<String>,
    /// The remote tunnel's owner, when this proxy may be reached through one
    /// (ADR 0012). The proxy asks it to redeem a pairing code, whether `/mcp`
    /// is open to tunnelled requests, and notes each tunnelled request on it.
    /// `None` for an embedded server, where nothing is listening for the
    /// answers.
    pub remote: Option<Arc<dyn RemoteGatewayPort>>,
    /// The daemon's ports a paired device reaches: its runs at `/v1/runs`
    /// and the hub's chats at `/v1/chats`. Empty for an embedded server:
    /// those routes then answer 503.
    pub devices: DevicePorts,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: DEFAULT_PROXY_PORT,
            default_context: None,
            cache_enabled: false,
            slot_dir: None,
            disk_budget: DiskBudget::Auto,
            inference_override: None,
            default_profile: None,
            api_key: None,
            daemon_cancel: None,
            allowed_hosts: Vec::new(),
            remote: None,
            devices: DevicePorts::default(),
        }
    }
}
