#![doc = include_str!("README.md")]

mod wire;

pub(crate) use wire::{ProxyStatus, StartPinnedBody, StartProxyConfig};

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};
use gglib_core::ports::AppEventEmitter;
use wire::{to_api_status, to_runtime_config};

/// Fetch current proxy status from backend.
async fn fetch_status(state: &AppState) -> ProxyStatus {
    let s = state.proxy.status().await;
    let pinned = state.proxy.pinned_model();
    to_api_status(s, pinned)
}

/// Start the proxy pinned to one model, resolving the launch cascade
/// server-side — the GUI counterpart of `gglib serve`.
pub(crate) async fn start_pinned(
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
        inference_override: body.proxy.inference_override.clone(),
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
        default_context: proxy_config.default_context,
        // Sampling rides the proxy-wide override, exactly as the CLI sends
        // it. It cannot ride the pinned model's launch options: gglib emits
        // no sampler flags to llama-server at all (ADR 0003/0004), so the
        // field those options write to is read by nobody. Hardcoding `None`
        // here was why a GUI pinned start discarded its own sampling.
        inference_override: proxy_config.inference_override,
        ..body.proxy
    };
    start(State(state), Json(Some(cfg))).await
}

/// Get current proxy status.
pub(crate) async fn status(State(state): State<AppState>) -> Json<ProxyStatus> {
    Json(fetch_status(&state).await)
}

/// Start the proxy (idempotent).
pub(crate) async fn start(
    State(state): State<AppState>,
    Json(cfg): Json<Option<StartProxyConfig>>,
) -> Result<Json<ProxyStatus>, HttpError> {
    let cfg = cfg.unwrap_or_default();

    // Resolve context size through the shared chain, matching CLI behaviour:
    // flag > settings default > nothing. Deliberately *not* down to the
    // hard-coded default — `to_runtime_config` collapses that rung to `None`
    // so the launch is sized by the daemon instead of arriving with a floor
    // nobody chose.
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

            // A running proxy's sampling override and default profile are
            // fixed for the life of the run — `AppState` reads both once at
            // startup. Asking for *different* ones would print the operator's
            // flags back at them and apply none of them, which is the precise
            // failure the override was wired up to fix.
            //
            // Compared rather than merely detected, because "the same request
            // again" is legitimate and common: re-running the identical
            // command is how an operator re-attaches after a detach, and with
            // `proxy_autostart` the daemon has already started an unpinned
            // proxy before the first `gglib proxy --temperature …` ever runs.
            // Refusing on presence alone broke both.
            if let Some((running_override, running_profile)) = state.proxy.started_sampling().await
                && (cfg.inference_override != running_override
                    || cfg.default_profile != running_profile)
            {
                return Err(HttpError::Conflict(
                    "the proxy is already running with different sampling — that is fixed for \
                     the life of the run, so stop it first (`gglib proxy stop`)"
                        .to_string(),
                ));
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
pub(crate) async fn stop(State(state): State<AppState>) -> Result<Json<ProxyStatus>, HttpError> {
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
