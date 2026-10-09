//! Route definitions and router construction.
//!
//! This module defines the HTTP routes and creates the main router.
//! Handlers delegate to the services on [`AppState`].

use axum::Json;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

use crate::access::DaemonAccess;
use crate::chat_api::chat_routes_no_prefix;
use crate::handlers;
use crate::state::AppState;
use crate::trust::{ApiCredentials, bearer_guard};
use gglib_core::CorsConfig;
use gglib_core::services::SettingsCache;
use gglib_proxy::access::{build_cors_layer, host_guard, origin_guard};

/// Build all API routes without `/api` prefix (for nesting under /api).
///
/// Returns a router typed as `Router<AppState>` (state inferred from handlers)
/// but WITHOUT `.with_state()` applied. The caller must apply `.with_state()` before
/// nesting. All endpoints are defined without the `/api` prefix since this router
/// will be nested under `/api` by the caller.
///
/// Routes are organized into domain groups:
/// - `/models/*`  — CRUD, tags, verification, downloads, `HuggingFace` discovery
/// - `/config/*`  — settings, system setup
/// - `/remote/*`  — the tunnel and the devices it admits (ADR 0012)
pub(crate) fn api_routes() -> Router<AppState> {
    Router::new()
        .nest("/models", model_routes())
        .nest("/config", config_routes())
        .nest("/remote", crate::routes_remote::remote_routes())
        .nest("/runs", crate::routes_runs::run_routes())
        // The images a message carries: sent once, named by id afterwards.
        .route(
            "/attachments",
            post(handlers::attachments::upload).layer(handlers::attachments::body_limit()),
        )
        .route("/attachments/{id}", get(handlers::attachments::fetch))
        .route("/version", get(handlers::version::get_version))
        // Servers API
        .route("/servers", get(handlers::servers::list))
        .route("/servers/start", post(handlers::servers::start_body))
        .route("/servers/stop", post(handlers::servers::stop_body))
        .route(
            "/servers/{id}/tool-support",
            get(handlers::servers::tool_support),
        )
        .route("/servers/{port}/logs", get(handlers::servers::get_logs))
        .route(
            "/servers/{port}/logs/stream",
            get(handlers::servers::stream_logs),
        )
        // Built-in tools API
        .route("/builtin/tools", get(handlers::builtin::list_builtin_tools))
        // MCP API
        .route(
            "/mcp/servers",
            get(handlers::mcp::list).post(handlers::mcp::add),
        )
        .route(
            "/mcp/servers/{id}",
            put(handlers::mcp::update).delete(handlers::mcp::remove),
        )
        .route("/mcp/servers/{id}/start", post(handlers::mcp::start))
        .route("/mcp/servers/{id}/stop", post(handlers::mcp::stop))
        .route(
            "/mcp/servers/{id}/resolve",
            post(handlers::mcp::resolve_path),
        )
        .route(
            "/mcp/servers/{id}/test",
            post(handlers::mcp::test_connection),
        )
        // Proxy API
        .route("/proxy/status", get(handlers::proxy::status))
        .route("/proxy/start", post(handlers::proxy::start))
        .route("/proxy/start-pinned", post(handlers::proxy::start_pinned))
        .route("/proxy/stop", post(handlers::proxy::stop))
        .route("/proxy/loop-guard-trips", get(handlers::proxy::trips))
        // Daemon lifecycle
        .route("/daemon/shutdown", post(handlers::daemon::shutdown))
        // Events (SSE), and the event a `gglib` command posts for a change
        // it made to the library in its own process.
        .route(
            "/events",
            get(handlers::events::stream).post(handlers::events::relay),
        )
        // Agent (server-side agentic loop with SSE streaming)
        //
        // Body limit: **4 MiB** (vs the Axum default of 2 MiB).
        //
        // Agent requests carry the full conversation history — every prior user
        // message, assistant turn, and tool result.  A typical turn adds ~2-4 KB
        // (prompt + tool JSON), so 4 MiB comfortably holds ~1 000 turns.  If you
        // place a reverse proxy (nginx, Caddy, …) in front of this server, make
        // sure its own body-size limit is at least 4 MiB as well, otherwise the
        // proxy will reject long sessions before Axum ever sees them.
        .route(
            "/agent/chat",
            post(handlers::agent::chat).layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .nest("/benchmark", crate::routes_benchmark::benchmark_routes())
        // Chat routes (merged without prefix since we're already building /api)
        .merge(chat_routes_no_prefix())
}

/// Model domain routes: CRUD, tags, verification, downloads, `HuggingFace`.
///
/// Nested under `/api/models` by the caller.
fn model_routes() -> Router<AppState> {
    Router::new()
        // CRUD
        .route(
            "/",
            get(handlers::model::models::list).post(handlers::model::models::add),
        )
        .route(
            "/{id}",
            get(handlers::model::models::get)
                .put(handlers::model::models::update)
                .patch(handlers::model::models::update)
                .delete(handlers::model::models::remove),
        )
        // Capability override: PATCH /api/models/{id}/capabilities
        // Independently set/clear individual ModelCapabilities flags without
        // touching any other model metadata.
        .route(
            "/{id}/capabilities",
            axum::routing::patch(handlers::model::models::set_capabilities),
        )
        .route("/{id}/retag", post(handlers::model::models::retag))
        .route(
            "/{id}/upgrade-check",
            get(handlers::model::models::check_upgrade),
        )
        .route(
            "/{id}/upgrade",
            post(handlers::model::models::apply_upgrade),
        )
        // Full inspect view: GET /api/models/{id}/detail
        // Returns ModelDetailDto — superset of GuiModel with raw GGUF metadata,
        // MoE topology, HuggingFace provenance, inference defaults, and timestamps.
        .route("/{id}/detail", get(handlers::model::models::detail))
        // Sampling provenance: GET /api/models/{id}/explain[?profile=NAME]
        // The resolved sampling parameters plus the layer that supplied each —
        // the HTTP form of `gglib model explain`.
        .route("/{id}/explain", get(handlers::model::models::explain))
        // Projector picker: GET /api/models/{id}/projectors
        // The files a PUT's `projectorPath` may name for this model.
        .route(
            "/{id}/projectors",
            get(handlers::model::models::projector_choices),
        )
        // Component pickers: GET /api/models/{id}/components
        // For each role an image model's family needs, the files a PUT's
        // `components` may name for it.
        .route(
            "/{id}/components",
            get(handlers::model::models::component_choices),
        )
        // Past raw-vs-gglib A/B reports for this model
        .route(
            "/{id}/agentic-history",
            get(handlers::benchmark::history::model_agentic_history),
        )
        // Tags
        .route(
            "/{id}/tags",
            get(handlers::model::models::get_model_tags)
                .post(handlers::model::models::add_tag_body),
        )
        .route(
            "/{id}/tags/{tag}",
            post(handlers::model::models::add_tag).delete(handlers::model::models::remove_tag),
        )
        .route("/tags", get(handlers::model::models::list_tags))
        .route(
            "/filter-options",
            get(handlers::model::models::filter_options),
        )
        // Verification
        .route("/{id}/verify", post(handlers::model::verification::verify))
        .route(
            "/{id}/updates",
            get(handlers::model::verification::check_updates),
        )
        .route("/{id}/repair", post(handlers::model::verification::repair))
        .merge(model_fetch_routes())
}

/// The download queue and `HuggingFace` discovery, merged into
/// [`model_routes`] and so under `/api/models` too.
fn model_fetch_routes() -> Router<AppState> {
    Router::new()
        // Downloads
        .route(
            // Two clients, not one: the dashboard polls this, and so does the
            // CLI's download monitor via
            // `gglib_core::contracts::http::daemon::DOWNLOADS_QUEUE_PATH`.
            // `tests/daemon_route_contract.rs` fails if this route moves.
            "/downloads/queue",
            get(handlers::model::downloads::list).post(handlers::model::downloads::queue),
        )
        .route(
            "/downloads/{id}",
            delete(handlers::model::downloads::remove),
        )
        .route(
            "/downloads/{id}/cancel",
            post(handlers::model::downloads::cancel),
        )
        .route(
            "/downloads/reorder",
            post(handlers::model::downloads::reorder),
        )
        .route(
            "/downloads/reorder-full",
            post(handlers::model::downloads::reorder_full),
        )
        // HuggingFace discovery
        .route("/hf/search", post(handlers::model::hf::search))
        .route(
            "/hf/model/{*model_id}",
            get(handlers::model::hf::model_summary),
        )
        .route(
            "/hf/quantizations/{model_id}",
            get(handlers::model::hf::quantizations),
        )
        .route(
            "/hf/tool-support/{model_id}",
            get(handlers::model::hf::tool_support),
        )
}

/// Config and system routes: settings, setup wizard.
///
/// Nested under `/api/config` by the caller.
fn config_routes() -> Router<AppState> {
    Router::new()
        // Settings
        .route(
            "/settings",
            get(handlers::config::settings::get)
                .put(handlers::config::settings::update)
                .patch(handlers::config::settings::update),
        )
        // The starter profiles, as `gglib config profile install-templates`
        // adds them without `--force`.
        .route(
            "/profiles/install-templates",
            post(handlers::config::settings::install_profile_templates),
        )
        // System
        .route("/system/memory", get(handlers::config::settings::memory))
        .route(
            "/system/models-directory",
            get(handlers::config::settings::models_directory)
                .put(handlers::config::settings::update_models_directory),
        )
        .route("/system/setup-status", get(handlers::config::setup::status))
        .route(
            "/system/install-llama",
            post(handlers::config::setup::install_llama),
        )
        .route(
            "/system/llama-status",
            get(handlers::config::setup::llama_status_handler),
        )
        // POST, not GET: this runs `git fetch`.
        .route(
            "/system/llama-check-updates",
            post(handlers::config::setup::check_llama_updates),
        )
        .route(
            "/system/update-llama",
            post(handlers::config::setup::update_llama),
        )
        .route(
            "/system/uninstall-llama",
            post(handlers::config::setup::uninstall_llama_handler),
        )
        .route(
            "/system/setup-python",
            post(handlers::config::setup::setup_python),
        )
        .route(
            "/system/disable-fast-downloads",
            post(handlers::config::setup::disable_fast_downloads),
        )
        .route(
            "/system/diagnostics",
            get(handlers::config::setup::diagnostics),
        )
        .route(
            "/system/recommend-model",
            get(handlers::config::setup::recommend_model),
        )
}

/// The router core shared by [`create_router`], [`create_spa_router`] and
/// [`crate::create_embedded_spa_router`]: `/health` plus `/api/*`, with CORS,
/// the origin guard and the bearer guard scoped to `/api/*`. The bearer guard
/// asks the daemon token, or the API key when the daemon bound with one.
///
/// The Host guard is *not* applied here — each public constructor layers it
/// last, after any fallback service, so it wraps everything the router will
/// ever serve.
pub(crate) fn base_router(state: AppState, cfg: &CorsConfig, access: &Arc<DaemonAccess>) -> Router {
    let cors = build_cors_layer(cfg);

    let settings = Arc::new(SettingsCache::new(state.core.settings().repo()));
    let credentials = ApiCredentials {
        policy: access.bearer_policy(settings),
        token: access.daemon_token().cloned(),
    };
    // A daemon that bound *with* a key follows a later rotation of it, and one
    // that bound without takes only the token and keeps taking only the token
    // — `DaemonAccess::bearer_policy` says why that asymmetry is the point.
    // /health stays outside the group: probes must not need
    // credentials. The origin guard reads the same config CORS does. CORS is
    // layered outside both guards so preflight OPTIONS requests, which never
    // carry Authorization, are answered by the CORS layer instead of a 401.
    let api = api_routes()
        .with_state(state)
        .layer(middleware::from_fn_with_state(credentials, bearer_guard))
        .layer(middleware::from_fn_with_state(
            Arc::new(cfg.clone()),
            origin_guard::<DaemonAccess>,
        ))
        .layer(cors);

    Router::new()
        // Intentionally placed outside the CORS layer — /health is a
        // low-sensitivity health probe that should be accessible without
        // origin restrictions (e.g. for container orchestration liveness checks).
        // The proxy server applies CORS globally (including /health) via a
        // router-level .layer(), but this Axum router scopes CORS to /api/* only.
        .route("/health", get(health_check))
        .nest("/api", api)
}

/// Create the main Axum router with all API routes.
///
/// This creates the API routes only. For serving static assets,
/// use [`create_spa_router`] which includes both API routes and
/// static file serving with SPA fallback.
///
/// `access` carries the Host allowlist (always enforced, on every route)
/// and the optional bearer token (enforced on `/api/*` when set).
///
/// # Path Parameter Syntax
/// Axum 0.8 uses brace syntax for path parameters: `{id}`, `{tag}`
pub fn create_router(
    state: AppState,
    cors_config: &CorsConfig,
    access: Arc<DaemonAccess>,
) -> Router {
    base_router(state, cors_config, &access).layer(middleware::from_fn_with_state(
        access,
        host_guard::<DaemonAccess>,
    ))
}

/// Create a router with API routes and static asset serving.
///
/// This creates a complete SPA-ready router that:
/// 1. Serves API routes under `/api/*` and `/health`
/// 2. Serves static assets from `static_dir` for matching files
/// 3. Falls back to `index.html` for client-side routing (SPA mode)
///
/// # Arguments
/// * `ctx` - The Axum context containing shared state
/// * `static_dir` - Path to the directory containing built frontend assets
/// * `cors_config` - CORS configuration
///
/// # Example
/// ```no_run
/// # use std::sync::Arc;
/// # use gglib_axum::{AppState, CorsConfig, DaemonAccess};
/// # async fn example(state: AppState) {
/// let access = Arc::new(DaemonAccess::loopback());
/// let router = gglib_axum::create_spa_router(state, "./dist", &CorsConfig::AllowAll, access);
/// # }
/// ```
pub fn create_spa_router<P: AsRef<Path>>(
    state: AppState,
    static_dir: P,
    cors_config: &CorsConfig,
    access: Arc<DaemonAccess>,
) -> Router {
    let static_path = static_dir.as_ref();
    let index_path = static_path.join("index.html");

    // Static file serving with SPA fallback to index.html for unmatched paths
    // Using .fallback() on ServeDir makes it return index.html for missing files
    let serve_dir = ServeDir::new(static_path).fallback(ServeFile::new(&index_path));

    // Merge API routes with static serving as fallback, then wrap the whole
    // thing — SPA assets included — in the Host guard. The layer must come
    // after the fallback so a rebound page cannot even load the dashboard.
    base_router(state, cors_config, &access)
        .fallback_service(serve_dir)
        .layer(middleware::from_fn_with_state(
            access,
            host_guard::<DaemonAccess>,
        ))
}

/// Health check endpoint.
///
/// Returns `{"service":"gglib-daemon","status":"ok","version":...}` so the
/// CLI daemon detection logic (Phase 3b) can confirm it is talking to a live
/// gglib daemon rather than an unrelated HTTP server on the same port. The
/// version lets clients detect a daemon left running from an older install.
pub(crate) async fn health_check() -> Json<Value> {
    Json(json!({
        "service": "gglib-daemon",
        "status": "ok",
        "version": gglib_build_info::SEMVER,
        // See `gglib_build_info::FINGERPRINT`: version alone cannot tell
        // two dev builds apart, and the probe warns on a mismatch.
        "fingerprint": gglib_build_info::FINGERPRINT,
        // Which `GGLIB_DISABLE_*` switches this daemon actually has in effect.
        //
        // Reported because the daemon is the process that reads them, and a
        // CLI invocation setting one gets no say — see
        // `gglib_core::debug_switches`. Without this the CLI has no way to
        // tell a switch it set from a switch that took effect, and a
        // debugging run silently measures the wrong thing.
        "debug_switches": gglib_core::debug_switches::active(),
    }))
}
