//! Proxy handlers - OpenAI-compatible proxy management.

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};
use gglib_app_services::types::AppSettings;
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
    /// The model this proxy run is pinned to, when started in pinned mode
    /// (`gglib serve`). `None` for the ordinary auto-swapping proxy.
    pub pinned_model: Option<String>,
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

/// Convert runtime ProxyStatus to API ProxyStatus.
fn to_api_status(s: RuntimeProxyStatus, pinned_model: Option<String>) -> ProxyStatus {
    match s {
        RuntimeProxyStatus::Stopped => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
            pinned_model: None,
        },
        RuntimeProxyStatus::Running { address } => ProxyStatus {
            running: true,
            port: Some(address.port()),
            current_model: None,
            model_port: None,
            pinned_model,
        },
        RuntimeProxyStatus::Crashed => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
            pinned_model: None,
        },
    }
}

/// Fetch current proxy status from backend.
async fn fetch_status(state: &AppState) -> ProxyStatus {
    let s = state.proxy.status().await;
    let pinned = state.proxy.pinned_model();
    to_api_status(s, pinned)
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
fn to_runtime_config(cfg: &StartProxyConfig, settings: &AppSettings) -> RuntimeProxyConfig {
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

/// Request body for `POST /api/proxy/start-pinned`.
///
/// The GUI names a model and its overrides; the daemon runs the same
/// cascade as `gglib serve` (`gglib_app_services::launch_options`) so the
/// two surfaces cannot drift. `options` uses the camelCase wire form of the
/// bare `/api/servers/start` body; `proxy` the snake_case form of
/// `/api/proxy/start`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StartPinnedBody {
    pub model_id: i64,
    #[serde(default)]
    pub options: gglib_app_services::types::StartServerRequest,
    #[serde(default)]
    pub proxy: StartProxyConfig,
}

/// Start the proxy pinned to one model, resolving the launch cascade
/// server-side — the GUI counterpart of `gglib serve`.
pub async fn start_pinned(
    State(state): State<AppState>,
    Json(body): Json<StartPinnedBody>,
) -> Result<Json<ProxyStatus>, HttpError> {
    let globals = gglib_app_services::launch_options::ProxyGlobals {
        host: body.proxy.host.clone(),
        proxy_port: body.proxy.port,
        llama_base_port: body.proxy.llama_base_port,
        default_ctx: body.proxy.default_context,
        cache_enabled: body.proxy.cache.unwrap_or(false),
        slot_dir: body.proxy.slot_dir.clone(),
        api_key: body.proxy.api_key.clone(),
        allowed_hosts: body.proxy.allowed_hosts.clone(),
    };

    let plan = state
        .proxy
        .plan_pinned(body.model_id, &body.options, globals)
        .await?;

    // Delegate to the ordinary start path with the planned pin. Slot dir and
    // default context ride the cascade exactly as the CLI sends them: the
    // master switch has been applied and the context fully resolved.
    let proxy_config = plan.unified.to_proxy_config();
    let cfg = StartProxyConfig {
        pinned: Some(plan.pinned),
        slot_dir: proxy_config.slot_dir,
        default_context: Some(proxy_config.default_context),
        // Sampling rides the pinned model's launch options, exactly as the
        // CLI sends it — never a proxy-wide override.
        inference_override: None,
        ..body.proxy
    };
    start(State(state), Json(Some(cfg))).await
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
    let runtime_cfg = to_runtime_config(&cfg, &settings);

    // Idempotent: if already running (Conflict), treat as success — unless
    // the caller asked for a pinned mode the running proxy does not match,
    // where "success" would silently hand them an endpoint without the
    // refuse-foreign-models guarantee they explicitly requested.
    match state.proxy.start(runtime_cfg, cfg.pinned.clone()).await {
        Ok(_addr) => {}
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }

            // `Conflict` covers two different outcomes. `AlreadyRunning` is
            // the idempotent success this branch exists for; `BindFailed` is
            // somebody else's process on the port, and it leaves nothing
            // running. Only the first is success, so ask which one happened
            // rather than assuming — treating a bind failure as success
            // answered 200 with `running: false` and discarded the one message
            // that named the port and what to do about it.
            if !fetch_status(&state).await.running {
                return Err(http);
            }

            let requested = cfg.pinned.as_ref().map(|p| p.name.clone());
            if requested.is_some() && state.proxy.pinned_model() != requested {
                return Err(HttpError::Conflict(format!(
                    "the proxy is already running {} — stop it first (`gglib proxy stop`)",
                    match state.proxy.pinned_model() {
                        Some(name) => format!("pinned to '{name}'"),
                        None => "unpinned".to_string(),
                    }
                )));
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
}
