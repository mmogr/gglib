//! Route definitions and router construction.
//!
//! This module defines the HTTP routes and creates the main router.
//! Handlers delegate to the shared GuiBackend facade.

use axum::Router;
use axum::routing::{delete, get, post, put};
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use crate::bootstrap::{AxumContext, CorsConfig};
use crate::chat_api::chat_routes_no_prefix;
use crate::handlers;
use crate::state::AppState;

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
    }
}

/// Build all API routes without `/api` prefix (for nesting under /api).
///
/// Returns a router typed as `Router<AppState>` (state inferred from handlers)
/// but WITHOUT `.with_state()` applied. The caller must apply `.with_state()` before
/// nesting. All endpoints are defined without the `/api` prefix since this router
/// will be nested under `/api` by the caller.
pub(crate) fn api_routes() -> Router<AppState> {
    Router::new()
        // Models API
        .route(
            "/models",
            get(handlers::models::list).post(handlers::models::add),
        )
        .route(
            "/models/{id}",
            get(handlers::models::get)
                .put(handlers::models::update)
                .delete(handlers::models::remove),
        )
        .route(
            "/models/{id}/tags",
            get(handlers::models::get_model_tags).post(handlers::models::add_tag_body),
        )
        .route(
            "/models/{id}/tags/{tag}",
            post(handlers::models::add_tag).delete(handlers::models::remove_tag),
        )
        .route(
            "/models/filter-options",
            get(handlers::models::filter_options),
        )
        // Model Verification API
        .route("/models/{id}/verify", post(handlers::verification::verify))
        .route(
            "/models/{id}/updates",
            get(handlers::verification::check_updates),
        )
        .route("/models/{id}/repair", post(handlers::verification::repair))
        // Tags API
        .route("/tags", get(handlers::models::list_tags))
        .route("/tags/{tag}/models", get(handlers::models::get_by_tag))
        // Settings API
        .route(
            "/settings",
            get(handlers::settings::get)
                .put(handlers::settings::update)
                .patch(handlers::settings::update),
        )
        .route("/system/memory", get(handlers::settings::memory))
        .route(
            "/system/models-directory",
            get(handlers::settings::models_directory)
                .put(handlers::settings::update_models_directory),
        )
        // Servers API
        .route("/servers", get(handlers::servers::list))
        .route("/servers/start", post(handlers::servers::start_body))
        .route("/servers/stop", post(handlers::servers::stop_body))
        .route("/servers/{id}/start", post(handlers::servers::start))
        .route("/servers/{id}/stop", post(handlers::servers::stop))
        .route(
            "/servers/{port}/logs",
            get(handlers::servers::get_logs).delete(handlers::servers::clear_logs),
        )
        .route(
            "/servers/{port}/logs/stream",
            get(handlers::servers::stream_logs),
        )
        // Downloads API
        .route("/downloads", get(handlers::downloads::list))
        .route(
            "/downloads/queue",
            get(handlers::downloads::list).post(handlers::downloads::queue),
        )
        .route("/downloads/{id}", delete(handlers::downloads::remove))
        .route("/downloads/{id}/cancel", post(handlers::downloads::cancel))
        .route("/downloads/reorder", post(handlers::downloads::reorder))
        .route(
            "/downloads/reorder-full",
            post(handlers::downloads::reorder_full),
        )
        .route(
            "/downloads/shard-group/{id}/cancel",
            post(handlers::downloads::cancel_shard_group),
        )
        .route(
            "/downloads/failed/clear",
            post(handlers::downloads::clear_failed),
        )
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
        .route("/mcp/servers/{id}/tools", get(handlers::mcp::list_tools))
        .route("/mcp/tools/call", post(handlers::mcp::call_tool))
        // Proxy API
        .route("/proxy/status", get(handlers::proxy::status))
        .route("/proxy/start", post(handlers::proxy::start))
        .route("/proxy/stop", post(handlers::proxy::stop))
        // Hugging Face API (strip /api prefix since we're nested under /api)
        .route("/hf/search", post(handlers::hf::search))
        .route("/hf/model/{*model_id}", get(handlers::hf::model_summary))
        .route(
            "/hf/quantizations/{model_id}",
            get(handlers::hf::quantizations),
        )
        .route(
            "/hf/tool-support/{model_id}",
            get(handlers::hf::tool_support),
        )
        // Events (SSE)
        .route("/events", get(handlers::events::stream))
        // Voice API
        .route("/voice/status", get(handlers::voice::status))
        .route("/voice/models", get(handlers::voice::list_models))
        .route(
            "/voice/models/stt/{id}/download",
            post(handlers::voice::download_stt_model),
        )
        .route(
            "/voice/models/tts/download",
            post(handlers::voice::download_tts_model),
        )
        .route(
            "/voice/models/vad/download",
            post(handlers::voice::download_vad_model),
        )
        .route("/voice/stt/load", post(handlers::voice::load_stt))
        .route("/voice/tts/load", post(handlers::voice::load_tts))
        .route("/voice/mode", put(handlers::voice::set_mode))
        .route("/voice/voice", put(handlers::voice::set_voice))
        .route("/voice/speed", put(handlers::voice::set_speed))
        .route("/voice/auto-speak", put(handlers::voice::set_auto_speak))
        .route("/voice/unload", post(handlers::voice::unload))
        .route("/voice/devices", get(handlers::voice::list_devices))
        // Audio I/O control (Phase 3 / PR 2)
        .route("/voice/start", post(handlers::voice::start))
        .route("/voice/stop", post(handlers::voice::stop))
        .route("/voice/ptt-start", post(handlers::voice::ptt_start))
        .route("/voice/ptt-stop", post(handlers::voice::ptt_stop))
        .route("/voice/speak", post(handlers::voice::speak))
        .route("/voice/stop-speaking", post(handlers::voice::stop_speaking))
        // Chat routes (merged without prefix since we're already building /api)
        .merge(chat_routes_no_prefix())
}

/// Create the main Axum router with all API routes.
///
/// This creates the API routes only. For serving static assets,
/// use [`create_spa_router`] which includes both API routes and
/// static file serving with SPA fallback.
///
/// # Path Parameter Syntax
/// Axum 0.8 uses brace syntax for path parameters: `{id}`, `{tag}`
pub fn create_router(ctx: AxumContext, cors_config: &CorsConfig) -> Router {
    let state: AppState = Arc::new(ctx);
    let cors = build_cors_layer(cors_config);

    Router::new()
        .route("/health", get(health_check))
        .nest("/api", api_routes().with_state(state).layer(cors))
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
/// ```ignore
/// let router = create_spa_router(ctx, "./dist", &CorsConfig::AllowAll);
/// ```
pub fn create_spa_router<P: AsRef<Path>>(
    ctx: AxumContext,
    static_dir: P,
    cors_config: &CorsConfig,
) -> Router {
    let static_path = static_dir.as_ref();
    let index_path = static_path.join("index.html");

    // Static file serving with SPA fallback to index.html for unmatched paths
    // Using .fallback() on ServeDir makes it return index.html for missing files
    let serve_dir = ServeDir::new(static_path).fallback(ServeFile::new(&index_path));

    // API routes (without fallback - they should 404 on unknown API paths)
    let api = create_router(ctx, cors_config);

    // Merge API routes with static serving as fallback
    // API routes take priority, then fallback to static/SPA serving
    api.fallback_service(serve_dir)
}

/// Health check endpoint.
pub(crate) async fn health_check() -> &'static str {
    "OK"
}
