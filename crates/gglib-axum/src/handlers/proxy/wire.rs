//! The proxy endpoints' request and response shapes, and their conversions.
//!
//! Split from the handlers so the resolution rules below — which are where the
//! surprises live, since an omitted field means "use what is configured"
//! rather than "use the compile-time default" — can be read and tested without
//! the routing around them.

use gglib_app_services::types::AppSettings;
use gglib_core::server_config::{
    ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};
use gglib_core::settings::DEFAULT_PROXY_PORT;
use gglib_runtime::proxy::ProxyConfig as RuntimeProxyConfig;
use gglib_runtime::proxy::ProxyStatus as RuntimeProxyStatus;

/// Proxy status response.
/// Matches Tauri's `ProxyStatus` for frontend compatibility.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub current_model: Option<String>,
    pub model_port: Option<u16>,
    /// The model this proxy run is pinned to, when started in pinned mode
    /// (`gglib serve`). `None` for the ordinary auto-swapping proxy.
    pub pinned_model: Option<String>,
}

/// Optional configuration for starting the proxy.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct StartProxyConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub llama_base_port: Option<u16>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub default_context: Option<u64>,
    /// Enable KV cache session persistence (disk slot save/restore),
    /// mirroring `gglib serve`/`gglib proxy --cache`. Omitted or `None`
    /// means disabled, matching the CLI's own default.
    #[serde(default)]
    pub cache: Option<bool>,
    /// Directory for KV cache slot files. Only consulted when `cache` is
    /// `true`; omitted falls back to `<data-root>/slots`, same as the CLI.
    #[serde(default)]
    pub slot_dir: Option<std::path::PathBuf>,
    /// Pin this proxy run to a single model (`gglib serve`). Carries the
    /// model name and its fully-cascaded launch options; every request for
    /// another model is refused while the pin holds. Omitted means the
    /// ordinary auto-swapping proxy.
    #[serde(default)]
    pub pinned: Option<gglib_core::ports::PinnedSpec>,
    /// Byte budget in GiB for the on-disk slot eviction sweep
    /// (`--cache-disk-gb`). Omitted auto-sizes from free disk space.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    #[serde(default)]
    pub cache_disk_gb: Option<u64>,
    /// Operator sampling overrides applied above the client's own request
    /// parameters (`gglib proxy --temperature …`, `gglib serve --temperature …`).
    #[serde(default)]
    pub inference_override: Option<gglib_core::domain::InferenceConfig>,
    /// Profile applied to requests that name the model without a
    /// `{model}:{profile}` suffix. Carried by name rather than resolved: the
    /// proxy re-reads profiles per request, so a name tracks live edits.
    #[serde(default)]
    pub default_profile: Option<String>,
    /// Bearer token demanded on `/v1/*` (`--api-key`). Omitted falls through
    /// to the stored setting.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Host header values to accept beyond loopback (`--allowed-host`).
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

/// Request body for `POST /api/proxy/start-pinned`.
///
/// The GUI names a model and its overrides; the daemon runs the same
/// cascade as `gglib serve` (`gglib_app_services::launch_options`) so the
/// two surfaces cannot drift. `options` uses the camelCase wire form of the
/// bare `/api/servers/start` body; `proxy` the snake_case form of
/// `/api/proxy/start`.
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct StartPinnedBody {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub model_id: i64,
    #[serde(default)]
    pub options: gglib_app_services::types::StartServerRequest,
    #[serde(default)]
    pub proxy: StartProxyConfig,
}

/// Convert runtime `ProxyStatus` to API `ProxyStatus`.
pub(super) fn to_api_status(s: RuntimeProxyStatus, pinned_model: Option<String>) -> ProxyStatus {
    match s {
        RuntimeProxyStatus::Running { address } => ProxyStatus {
            running: true,
            port: Some(address.port()),
            current_model: None,
            model_port: None,
            pinned_model,
        },
        // A crashed proxy is not serving, so it reports exactly as a stopped
        // one does: no port to hand out, and no pin still in force.
        RuntimeProxyStatus::Stopped | RuntimeProxyStatus::Crashed => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
            pinned_model: None,
        },
    }
}

/// Convert handler config to runtime config, resolving anything omitted from
/// the user's saved settings.
///
/// An omitted field means "use what is configured", not "use the compile-time
/// default" — a caller that sends no port, as the tray panel does, must land on
/// the same port as the desktop app, `gglib proxy` and
/// `ProxyOps::ensure_running`. Going straight to `DEFAULT_PROXY_PORT` here
/// would silently ignore a changed `proxy_port` for every client of this
/// endpoint.
pub(super) fn to_runtime_config(
    cfg: &StartProxyConfig,
    settings: &AppSettings,
) -> RuntimeProxyConfig {
    // Only a value somebody chose. Resolving to the built-in floor here would
    // hand the proxy `Some(4096)` and make the fitted rung unreachable.
    let default_context = match resolve_context_size_with_source(&ServerConfigOptions {
        context_size: cfg.default_context,
        global_default_ctx: settings.default_context_size,
        ..Default::default()
    }) {
        (_, ContextSizeSource::BuiltInDefault) => None,
        (ctx, _) => Some(ctx),
    };

    let cache_enabled = cfg.cache.unwrap_or(false);
    // Resolved here rather than left `None`: the Axum proxy path errors
    // requests with "slot_dir not configured" when cache_enabled is true and
    // slot_dir is absent (see gglib-proxy's server.rs), unlike the CLI's
    // standalone path, which auto-defaults it. Applying the same default
    // here keeps `cache: true` alone sufficient, matching the CLI.
    let slot_dir = cache_enabled.then(|| {
        cfg.slot_dir
            .clone()
            .unwrap_or_else(gglib_runtime::default_slot_dir)
    });

    RuntimeProxyConfig {
        host: cfg.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
        // Same fallback chain as `Settings::effective_proxy_port`.
        port: cfg
            .port
            .or(settings.proxy_port)
            .unwrap_or(DEFAULT_PROXY_PORT),
        default_context,
        cache_enabled,
        slot_dir,
        disk_budget: gglib_runtime::proxy::resolve_disk_budget(cfg.cache_disk_gb),
        inference_override: cfg.inference_override.clone(),
        default_profile: cfg.default_profile.clone(),
        api_key: cfg.api_key.clone(),
        allowed_hosts: cfg.allowed_hosts.clone(),
        // Attached by `ProxyOps` on the way through, which is the one place
        // that knows whether a daemon is running this.
        daemon_cancel: None,
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
