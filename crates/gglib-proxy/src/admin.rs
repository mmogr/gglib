//! Administrative routes: clearing the caches, and stopping the daemon.
//!
//! Both sit inside the proxy's bearer-guarded group, and they are here rather
//! than in `server.rs` because that file is the composition root — it builds
//! the state and the router, and a request handler with a hundred lines of
//! its own behaviour is a second thing for it to be.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::cache_lifecycle::clear_cache;
use crate::server::AppState;

/// Handle cache clear requests via `POST /v1/proxy/cache/clear`.
///
/// Two independent caches sit behind this endpoint:
///
/// * the **disk slot** layer, opt-in via `--cache`, cleared per-session with
///   `X-Gglib-Session-Id` or wholesale without it;
/// * llama-server's **host-RAM prompt cache** (`--cache-ram`), which has no
///   clear API of its own — recycling the process is the only way to drop it.
///
/// A global clear therefore also recycles the model. Without that, the common
/// configuration (RAM cache on, disk layer off) had no way to clear the only
/// cache it actually had: the endpoint reported `cache not enabled` and did
/// nothing, which is the least useful answer available.
///
/// A session-scoped clear is deliberately disk-only. Recycling the process to
/// service one session would discard every other session's cached prefix too.
pub(crate) async fn handle_proxy_cache_clear(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Extract optional session ID from header
    let session_id = headers
        .get("x-gglib-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Sanitize if provided — 400 on invalid input (safety-critical)
    if let Some(ref sid) = session_id
        && let Err(e) = crate::slots::sanitize_session_id(sid)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("invalid session id: {}", e)
            })),
        );
    }

    // ── Disk slot layer ───────────────────────────────────────────────────
    let disk = if state.cache_enabled {
        // base_url is unused by clear_cache; model_id 0 is a sentinel — it only
        // touches flags and hot-cache invalidation, not any specific model's slots.
        let Some(config) = state.build_stream_config(String::new(), 0) else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "slot_dir not configured"
                })),
            );
        };
        match clear_cache(&config, session_id.as_deref()).await {
            Ok(()) => {
                if session_id.is_some() {
                    "session cleared"
                } else {
                    "all slots cleared"
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    } else {
        "disk cache not enabled"
    };

    // ── Host-RAM prompt cache ─────────────────────────────────────────────
    let ram = if session_id.is_some() {
        "RAM cache kept (session-scoped clear)"
    } else if state.dashboard.connections.is_empty() {
        // Gated on idle for the same reason as the watchdog recycle in
        // `chat_completions`: with `--parallel 1` an in-flight request owns the
        // only slot, and stop_current() would kill its live generation.
        info!("cache clear: recycling model to flush the host-RAM prompt cache");
        match state.runtime_port.stop_current().await {
            Ok(()) => "model recycled, RAM cache flushed",
            Err(e) => {
                warn!(error = %e, "cache clear: model recycle failed");
                "RAM cache not flushed (recycle failed)"
            }
        }
    } else {
        "RAM cache not flushed (request in flight; retry when idle)"
    };

    // Drop any frozen per-session calibration snapshot too, so a session
    // that explicitly cleared its cache re-baselines from the current
    // global ratio on its next request instead of reusing a pre-clear one.
    // Re-derive the sanitized (lowercased) form rather than reusing the raw
    // header value above — the validation call earlier only checks
    // `sanitize_session_id`'s `Err` case and discards its `Ok(String)`, so
    // `session_id` itself is still whatever case the client sent, and
    // `chat_completions` always keys snapshots by the lowercased form.
    if let Some(ref sid) = session_id {
        if let Ok(sanitized) = crate::slots::sanitize_session_id(sid) {
            state.calibration.clear_session(&sanitized);
        }
    } else {
        state.calibration.clear_all_sessions();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "message": format!("{disk}; {ram}"),
            "disk": disk,
            "ram": ram,
        })),
    )
}
/// What `POST /v1/proxy/shutdown` requires in its body.
#[derive(Debug, Deserialize)]
pub(crate) struct ShutdownRequest {
    /// Must be the literal `"shutdown"`.
    confirm: Option<String>,
}

/// The word the body has to carry.
const CONFIRMATION: &str = "shutdown";

/// Stop the whole daemon, from a client that can prove it holds the key.
///
/// # Why this lives on the proxy rather than the daemon port
///
/// The daemon already has `POST /api/daemon/shutdown`, and it is unreachable
/// from where this matters. A remote client arrives through a tunnel that
/// forwards to exactly one backend — this proxy — so port 9887 is not
/// somewhere it can go. Leaving the only stop button there means an endpoint
/// that can be reached from anywhere and switched off from nowhere.
///
/// Everything in the protected group requires the bearer token, this route
/// included. That is the whole access story: whoever can run inference here
/// can also stop it, and nobody else can do either.
///
/// # Why it asks twice
///
/// **This is a one-way door.** Once the daemon is down, nothing brings it back
/// except physical access to the machine — which is precisely the situation
/// the person firing it is trying to get out of. A bare `POST` is too easy to
/// arrive at by accident: a retried request, a prefetch, a shell history entry
/// recalled one line off. So the body must carry `{"confirm":"shutdown"}`, and
/// anything else is a 400 that changes nothing.
///
/// # Why the whole daemon
///
/// Stopping only the tunnel would leave the models loaded and the machine
/// answering on its LAN, which is the lesser half of what "stop" means when
/// you have decided to stop. Cancelling the daemon's token runs the shutdown
/// it already performs for a local `gglib daemon stop`: the proxy, then the
/// model servers, then the downloads, under the force-exit watchdog that
/// already exists.
pub(crate) async fn handle_proxy_shutdown(
    State(state): State<AppState>,
    body: Option<Json<ShutdownRequest>>,
) -> impl IntoResponse {
    let confirmed = body
        .and_then(|Json(req)| req.confirm)
        .is_some_and(|word| word == CONFIRMATION);
    if !confirmed {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "shutting down is not reversible from here — send {{\"confirm\":\"{CONFIRMATION}\"}} to mean it"
                ),
            })),
        );
    }

    // Absent when this proxy is not running under the daemon — an embedded
    // server, or a test. Saying so is better than reporting a stop that
    // nothing would carry out.
    let Some(token) = state.daemon_shutdown() else {
        warn!("remote shutdown requested, but this proxy is not running under the daemon");
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "this proxy is not running under the gglib daemon",
            })),
        );
    };

    info!("remote shutdown requested by an authenticated client; stopping the daemon");
    token.cancel();
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "stopping": true })),
    )
}
