//! Proxy handlers - OpenAI-compatible proxy management.

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};
use gglib_core::ports::AppEventEmitter;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::settings::DEFAULT_PROXY_PORT;
use gglib_runtime::proxy::ProxyConfig as RuntimeProxyConfig;
use gglib_runtime::proxy::ProxyStatus as RuntimeProxyStatus;

/// Proxy status response.
/// Matches Tauri's ProxyStatus for frontend compatibility.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub current_model: Option<String>,
    pub model_port: Option<u16>,
}

/// Optional configuration for starting the proxy.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StartProxyConfig {
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
}

/// Convert runtime ProxyStatus to API ProxyStatus.
fn to_api_status(s: RuntimeProxyStatus) -> ProxyStatus {
    match s {
        RuntimeProxyStatus::Stopped => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
        },
        RuntimeProxyStatus::Running { address } => ProxyStatus {
            running: true,
            port: Some(address.port()),
            current_model: None,
            model_port: None,
        },
        RuntimeProxyStatus::Crashed => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
        },
    }
}

/// Fetch current proxy status from backend.
async fn fetch_status(state: &AppState) -> ProxyStatus {
    let s = state.proxy.status().await;
    to_api_status(s)
}

/// Convert handler config to runtime config with defaults.
fn to_runtime_config(cfg: &StartProxyConfig, settings_default: Option<u64>) -> RuntimeProxyConfig {
    let default_context = resolve_context_size(&ServerConfigOptions {
        context_size: cfg.default_context,
        global_default_ctx: settings_default,
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
        port: cfg.port.unwrap_or(DEFAULT_PROXY_PORT),
        default_context,
        cache_enabled,
        slot_dir,
        ..Default::default()
    }
}

/// Get current proxy status.
pub async fn status(State(state): State<AppState>) -> Json<ProxyStatus> {
    Json(fetch_status(&state).await)
}

/// Start the proxy (idempotent).
pub async fn start(
    State(state): State<AppState>,
    Json(cfg): Json<Option<StartProxyConfig>>,
) -> Result<Json<ProxyStatus>, HttpError> {
    let cfg = cfg.unwrap_or_default();

    // Resolve context size through the shared 3-level fallback chain
    // (flag > settings default > hard-coded default), matching CLI behavior.
    let settings = state.settings.get().await?;
    let runtime_cfg = to_runtime_config(&cfg, settings.default_context_size);

    // Idempotent: if already running (Conflict), treat as success
    match state.proxy.start(runtime_cfg).await {
        Ok(_addr) => {}
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    let status = fetch_status(&state).await;

    // Emit proxy started event if proxy is now running
    if status.running
        && let Some(port) = status.port
    {
        state
            .sse
            .emit(gglib_core::events::AppEvent::proxy_started(port));
    }

    Ok(Json(status))
}

/// Stop the proxy (idempotent).
pub async fn stop(State(state): State<AppState>) -> Result<Json<ProxyStatus>, HttpError> {
    // Idempotent: if not running (Conflict), treat as success
    match state.proxy.stop().await {
        Ok(()) => {
            // Emit proxy stopped event on clean shutdown
            state
                .sse
                .emit(gglib_core::events::AppEvent::proxy_stopped());
        }
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    Ok(Json(fetch_status(&state).await))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Omitting `cache` must mean disabled, matching the CLI's own default —
    /// and, with cache off, no slot dir even if one were supplied.
    #[test]
    fn cache_omitted_defaults_to_disabled() {
        let cfg = StartProxyConfig::default();
        let runtime_cfg = to_runtime_config(&cfg, None);
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
        let runtime_cfg = to_runtime_config(&cfg, None);
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
        let runtime_cfg = to_runtime_config(&cfg, None);
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
        let runtime_cfg = to_runtime_config(&cfg, None);
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
        let runtime_cfg = to_runtime_config(&cfg, Some(16_384));
        assert_eq!(runtime_cfg.default_context, 16_384);
    }
}
