//! The proxy endpoints' request and response shapes, and their conversions.
//!
//! Split from the handlers so the resolution rules below — which are where the
//! surprises live, since an omitted field means "use what is configured"
//! rather than "use the compile-time default" — can be read and tested without
//! the routing around them.

use gglib_app_services::types::AppSettings;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_PROXY_PORT;
use gglib_runtime::proxy::ProxyConfig as RuntimeProxyConfig;
use gglib_runtime::proxy::ProxyStatus as RuntimeProxyStatus;

/// Proxy status response.
/// Matches Tauri's `ProxyStatus` for frontend compatibility.
#[derive(Debug, Clone, serde::Serialize)]
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
pub(crate) struct StartProxyConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub llama_base_port: Option<u16>,
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
    #[serde(default)]
    pub cache_disk_gb: Option<u64>,
    /// Operator sampling overrides applied above the client's own request
    /// parameters (`gglib proxy --temperature …`).
    #[serde(default)]
    pub inference_override: Option<gglib_core::domain::InferenceConfig>,
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
pub(crate) struct StartPinnedBody {
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
    let default_context = resolve_context_size(&ServerConfigOptions {
        context_size: cfg.default_context,
        global_default_ctx: settings.default_context_size,
        ..Default::default()
    });

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
        api_key: cfg.api_key.clone(),
        allowed_hosts: cfg.allowed_hosts.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An omitted port must come from settings, not from the compile-time
    /// default. The tray panel sends no port at all, and starting it on 8080
    /// while every other surface used the configured port is exactly the
    /// split-brain this endpoint has to avoid.
    #[test]
    fn an_omitted_port_comes_from_settings() {
        let settings = AppSettings {
            proxy_port: Some(18080),
            ..AppSettings::default()
        };
        let runtime_cfg = to_runtime_config(&StartProxyConfig::default(), &settings);
        assert_eq!(runtime_cfg.port, 18080);
    }

    /// An explicit port still wins: settings are the fallback, not an override.
    #[test]
    fn an_explicit_port_beats_the_setting() {
        let settings = AppSettings {
            proxy_port: Some(18080),
            ..AppSettings::default()
        };
        let cfg = StartProxyConfig {
            port: Some(9999),
            ..Default::default()
        };
        assert_eq!(to_runtime_config(&cfg, &settings).port, 9999);
    }

    /// With neither a request port nor a saved one, the hard-coded default is
    /// still the floor.
    #[test]
    fn no_port_anywhere_falls_back_to_the_default() {
        let runtime_cfg = to_runtime_config(&StartProxyConfig::default(), &AppSettings::default());
        assert_eq!(runtime_cfg.port, DEFAULT_PROXY_PORT);
    }

    /// Omitting `cache` must mean disabled, matching the CLI's own default —
    /// and, with cache off, no slot dir even if one were supplied.
    #[test]
    fn cache_omitted_defaults_to_disabled() {
        let cfg = StartProxyConfig::default();
        let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
        assert!(!runtime_cfg.cache_enabled);
        assert_eq!(runtime_cfg.slot_dir, None);
    }

    /// The master switch beats an explicit slot dir, matching
    /// `ProxyCacheOptions`/`UnifiedServerConfig` on the CLI side: cache off
    /// means zero cache-related settings reach the runtime, full stop.
    #[test]
    fn cache_false_ignores_a_supplied_slot_dir() {
        let cfg = StartProxyConfig {
            cache: Some(false),
            slot_dir: Some(std::path::PathBuf::from("/custom/slots")),
            ..Default::default()
        };
        let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
        assert!(!runtime_cfg.cache_enabled);
        assert_eq!(runtime_cfg.slot_dir, None);
    }

    /// `cache: true` with an explicit directory must carry it through
    /// unchanged — this is what lets a GUI-started proxy persist KV slots
    /// anywhere but the default location.
    #[test]
    fn cache_true_carries_the_explicit_slot_dir() {
        let cfg = StartProxyConfig {
            cache: Some(true),
            slot_dir: Some(std::path::PathBuf::from("/custom/slots")),
            ..Default::default()
        };
        let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
        assert!(runtime_cfg.cache_enabled);
        assert_eq!(
            runtime_cfg.slot_dir,
            Some(std::path::PathBuf::from("/custom/slots"))
        );
    }

    /// `cache: true` with no directory must fall back to the same default the
    /// CLI uses, not `None` — the Axum proxy path errors requests with
    /// "slot_dir not configured" when cache is on and slot_dir is absent, so
    /// leaving it `None` here would make `cache: true` alone insufficient.
    #[test]
    fn cache_true_without_slot_dir_uses_the_default_directory() {
        let cfg = StartProxyConfig {
            cache: Some(true),
            ..Default::default()
        };
        let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
        assert!(runtime_cfg.cache_enabled);
        assert_eq!(
            runtime_cfg.slot_dir,
            Some(gglib_runtime::default_slot_dir())
        );
    }

    /// `default_context` resolution is untouched by the cache wiring — still
    /// falls through explicit → settings → hardcoded default.
    #[test]
    fn default_context_falls_through_to_settings() {
        let cfg = StartProxyConfig::default();
        let settings = AppSettings {
            default_context_size: Some(16_384),
            ..AppSettings::default()
        };
        let runtime_cfg = to_runtime_config(&cfg, &settings);
        assert_eq!(runtime_cfg.default_context, 16_384);
    }

    /// A crashed proxy must not look reachable. It reports stopped, with no
    /// port for a client to be handed and no pin left standing.
    #[test]
    fn a_crashed_proxy_reports_as_stopped() {
        let status = to_api_status(RuntimeProxyStatus::Crashed, Some("qwen".to_owned()));

        assert!(!status.running);
        assert_eq!(status.port, None);
        assert_eq!(status.pinned_model, None);
    }
}
