//! The proxy's router: which routes exist, and which guards wrap which.
//!
//! Split from `server.rs`, which is the composition root and was at its
//! file-size ceiling, when the remote tunnel's routes arrived. Everything
//! about *layer order* lives here and is the whole of what this file
//! decides; the handlers live where their subject does.
//!
//! ```text
//! CorsLayer              outermost — answers OPTIONS preflight itself
//!   host_guard           every route, every path, always on
//!     remote_marker      every route: is this request tunnelled?
//!       (route match)
//!         bearer_guard   the protected group only
//!           mcp_tunnel_guard   /mcp only: tunnelled ⇒ needs --allow-mcp
//!             handler
//! ```
//!
//! `/health` and `/v1/remote/pair` sit outside the bearer group on purpose:
//! the first is polled before anyone has credentials, and the second is
//! how credentials are obtained — it cannot demand the key it hands out. Both
//! are still behind the Host guard, and the pairing route is behind the
//! tunnel edge's own one-time grant besides.

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use gglib_core::access::{ApiKeySource, BearerPolicy};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::mcp::handlers::{delete_mcp, get_mcp, post_mcp};
use crate::server::{
    AppState, chat_completions, handle_proxy_status, handle_proxy_status_stream, health_check,
};

/// Assemble the router over a built [`AppState`].
pub(crate) fn build(state: AppState, access: &ProxyAccessConfig) -> Router {
    // `/mcp` carries one guard of its own. A request that arrived through the
    // tunnel may not reach it unless the tunnel's owner said so, because
    // `invoke_tool` starts the MCP servers configured on this machine — a
    // leaked token with a shell server configured is remote code execution,
    // not free inference (ADR 0012).
    let mcp = Router::new()
        .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::remote::mcp_tunnel_guard,
        ));

    // Everything a client can reach with credentials. Grouped separately from
    // `/health` so `route_layer` can require the bearer token here without
    // closing the one endpoint a supervisor or a load balancer needs to poll
    // before it has any credentials to poll with.
    let mut protected = Router::new()
        .route("/v1/models", get(crate::models_endpoint::list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(crate::embeddings::embeddings))
        .route("/v1/proxy/status", get(handle_proxy_status))
        .route("/v1/proxy/status/stream", get(handle_proxy_status_stream))
        .route(
            "/v1/proxy/cache/clear",
            post(crate::admin::handle_proxy_cache_clear),
        )
        .route(
            "/v1/proxy/shutdown",
            post(crate::admin::handle_proxy_shutdown),
        )
        .merge(mcp);

    // Unconditional: a key set after this process started has to have a layer
    // to be enforced by. Which key it is, and whether one is required at all,
    // is re-decided per request from the settings cache below.
    let policy = match access.api_key_source {
        ApiKeySource::Flag => BearerPolicy::pinned(access.api_key.as_deref().unwrap_or_default()),
        _ => BearerPolicy::tracking(access.api_key.as_deref(), Arc::clone(&state.settings)),
    };
    protected = protected.route_layer(axum::middleware::from_fn_with_state(
        policy,
        crate::access::bearer_guard,
    ));

    Router::new()
        .route("/health", get(health_check))
        // Outside the bearer group: this is how the key is obtained. Inside
        // the Host guard, and behind the tunnel edge's one-time grant.
        .route("/v1/remote/pair", post(crate::remote::handle_remote_pair))
        .merge(protected)
        // Reads the tunnel markers the serve side sets, and records the
        // request as tunnelled for the gate above and the status surface.
        // Inside the Host guard: a request the guard refuses is not counted.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::remote::remote_marker,
        ))
        // Host allowlist: always on, and outside the router so it covers
        // `/health` and unmatched paths too. This is the DNS-rebinding guard;
        // see the `access` module for why CORS alone does not cover it.
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(access.clone()),
            crate::access::host_guard,
        ))
        // LocalOnly CORS: mirrors the Axum web server's default security posture.
        // Only localhost, 127.0.0.1, ::1, and tauri://localhost origins are accepted.
        //
        // Outermost deliberately: it answers OPTIONS preflight itself, and a
        // preflight that reached the guards above would be refused for carrying
        // credentials it is not allowed to carry yet.
        .layer(build_cors_layer(&access.cors))
        .with_state(state)
}

/// Build CORS layer from configuration.
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    match config {
        CorsConfig::AllowAll => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
        CorsConfig::AllowOrigins(origins) => {
            use axum::http::HeaderValue;
            let allowed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
            CorsLayer::new()
                .allow_origin(allowed)
                .allow_methods(Any)
                .allow_headers(Any)
        }
        CorsConfig::LocalOnly => {
            let local = AllowOrigin::predicate(|origin: &axum::http::HeaderValue, _req_headers| {
                gglib_core::is_local_origin(origin.to_str().unwrap_or(""))
            });
            CorsLayer::new()
                .allow_origin(local)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    }
}
