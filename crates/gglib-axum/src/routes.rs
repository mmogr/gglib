//! Route definitions and router construction.
//!
//! This module defines the HTTP routes and creates the main router.
//! Handlers delegate to the shared GuiBackend facade.

use axum::Router;
use axum::extract::DefaultBodyLimit;
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
///
/// Routes are organized into domain groups:
/// - `/models/*`  — CRUD, tags, verification, downloads, HuggingFace discovery
/// - `/config/*`  — settings, system setup
pub(crate) fn api_routes() -> Router<AppState> {
    Router::new()
        .nest("/models", model_routes())
        .nest("/config", config_routes())
        // Servers API
        .route("/servers", get(handlers::servers::list))
        .route("/servers/start", post(handlers::servers::start_body))
        .route("/servers/stop", post(handlers::servers::stop_body))
        .route("/servers/{id}/start", post(handlers::servers::start))
        .route("/servers/{id}/stop", post(handlers::servers::stop))
        .route(
            "/servers/{id}/tool-support",
            get(handlers::servers::tool_support),
        )
        .route(
            "/servers/{port}/logs",
            get(handlers::servers::get_logs).delete(handlers::servers::clear_logs),
        )
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
        .route("/mcp/servers/{id}/tools", get(handlers::mcp::list_tools))
        .route("/mcp/tools/call", post(handlers::mcp::call_tool))
        // Proxy API
        .route("/proxy/status", get(handlers::proxy::status))
        .route("/proxy/start", post(handlers::proxy::start))
        .route("/proxy/stop", post(handlers::proxy::stop))
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
        // Audio I/O control
        .route("/voice/start", post(handlers::voice::start))
        .route("/voice/stop", post(handlers::voice::stop))
        .route("/voice/ptt-start", post(handlers::voice::ptt_start))
        .route("/voice/ptt-stop", post(handlers::voice::ptt_stop))
        .route("/voice/speak", post(handlers::voice::speak))
        .route("/voice/stop-speaking", post(handlers::voice::stop_speaking))
        // WebSocket audio data plane
        .route("/voice/audio", get(handlers::voice_ws::audio_ws))
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
        // Council (multi-agent deliberation)
        .route("/council/suggest", post(handlers::council::suggest))
        .route("/council/run", post(handlers::council::run))
        // Chat routes (merged without prefix since we're already building /api)
        .merge(chat_routes_no_prefix())
}

/// Model domain routes: CRUD, tags, verification, downloads, HuggingFace.
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
                .delete(handlers::model::models::remove),
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
        .route("/tags/{tag}", get(handlers::model::models::get_by_tag))
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
        // Downloads
        .route("/downloads", get(handlers::model::downloads::list))
        .route(
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
        .route(
            "/downloads/shard-group/{id}/cancel",
            post(handlers::model::downloads::cancel_shard_group),
        )
        .route(
            "/downloads/failed/clear",
            post(handlers::model::downloads::clear_failed),
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
        // System
        .route("/system/memory", get(handlers::config::settings::memory))
        .route(
            "/system/models-directory",
            get(handlers::config::settings::models_directory)
                .put(handlers::config::settings::update_models_directory),
        )
        .route("/system/setup-status", get(handlers::config::setup::status))
        .route(
            "/system/vulkan-status",
            get(handlers::config::setup::vulkan_status_handler),
        )
        .route(
            "/system/install-llama",
            post(handlers::config::setup::install_llama),
        )
        .route(
            "/system/build-llama-from-source",
            post(handlers::config::setup::build_llama_from_source),
        )
        .route(
            "/system/setup-python",
            post(handlers::config::setup::setup_python),
        )
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
