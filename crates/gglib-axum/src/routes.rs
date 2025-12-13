//! Route definitions and router construction.
//!
//! This module defines the HTTP routes and creates the main router.
//! Handlers delegate to the shared GuiBackend facade.

use axum::Router;
use axum::routing::{delete, get, post, put};
use gglib_core::contracts::http::hf as hf_routes;
use std::path::Path;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use crate::bootstrap::{AxumContext, CorsConfig};
use crate::chat_api::{ChatApiContext, chat_routes};
use crate::handlers;

/// Application state shared across handlers.
pub type AppState = Arc<AxumContext>;

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

/// Create the main Axum router with all API routes.
///
/// This creates the API routes only. For serving static assets,
/// use [`create_spa_router`] which includes both API routes and
/// static file serving with SPA fallback.
///
/// # Path Parameter Syntax
/// Axum 0.7 uses colon syntax for path parameters: `:id`, `:tag`
/// (Axum 0.8+ uses brace syntax: `{id}`, `{tag}`)
pub fn create_router(ctx: AxumContext, cors_config: &CorsConfig) -> Router {
    let state: AppState = Arc::new(ctx);
    let cors = build_cors_layer(cors_config);

    // Build non-chat API routes and bind AppState
    let app_routes = Router::new()
        // Health check endpoint
        .route("/health", get(health_check))
        // Models API
        .route(
            "/api/models",
            get(handlers::models::list).post(handlers::models::add),
        )
        .route(
            "/api/models/:id",
            get(handlers::models::get)
                .put(handlers::models::update)
                .delete(handlers::models::remove),
        )
        .route(
            "/api/models/:id/tags",
            get(handlers::models::get_model_tags).post(handlers::models::add_tag_body),
        )
        .route(
            "/api/models/:id/tags/:tag",
            post(handlers::models::add_tag).delete(handlers::models::remove_tag),
        )
        .route(
            "/api/models/filter-options",
            get(handlers::models::filter_options),
        )
        // Tags API
        .route("/api/tags", get(handlers::models::list_tags))
        .route("/api/tags/:tag/models", get(handlers::models::get_by_tag))
        // Settings API
        // Accept both PUT and PATCH - frontend sends PUT, standard REST uses PATCH for partial updates
        .route(
            "/api/settings",
            get(handlers::settings::get)
                .put(handlers::settings::update)
                .patch(handlers::settings::update),
        )
        .route("/api/system/memory", get(handlers::settings::memory))
        .route(
            "/api/system/models-directory",
            get(handlers::settings::models_directory)
                .put(handlers::settings::update_models_directory),
        )
        // Servers API
        // Collection routes (body-based) - matches frontend transport
        .route("/api/servers", get(handlers::servers::list))
        .route("/api/servers/start", post(handlers::servers::start_body))
        .route("/api/servers/stop", post(handlers::servers::stop_body))
        // Resource routes (path-based) - legacy/alternative access
        .route("/api/servers/:id/start", post(handlers::servers::start))
        .route("/api/servers/:id/stop", post(handlers::servers::stop))
        // Downloads API
        .route("/api/downloads", get(handlers::downloads::list))
        .route(
            "/api/downloads/queue",
            get(handlers::downloads::list).post(handlers::downloads::queue),
        )
        .route("/api/downloads/:id", delete(handlers::downloads::remove))
        .route(
            "/api/downloads/:id/cancel",
            post(handlers::downloads::cancel),
        )
        .route("/api/downloads/reorder", post(handlers::downloads::reorder))
        .route(
            "/api/downloads/reorder-full",
            post(handlers::downloads::reorder_full),
        )
        .route(
            "/api/downloads/shard-group/:id/cancel",
            post(handlers::downloads::cancel_shard_group),
        )
        .route(
            "/api/downloads/failed/clear",
            post(handlers::downloads::clear_failed),
        )
        // MCP API
        .route(
            "/api/mcp/servers",
            get(handlers::mcp::list).post(handlers::mcp::add),
        )
        .route(
            "/api/mcp/servers/:id",
            put(handlers::mcp::update).delete(handlers::mcp::remove),
        )
        .route("/api/mcp/servers/:id/start", post(handlers::mcp::start))
        .route("/api/mcp/servers/:id/stop", post(handlers::mcp::stop))
        .route("/api/mcp/servers/:id/tools", get(handlers::mcp::list_tools))
        .route("/api/mcp/tools/call", post(handlers::mcp::call_tool))
        // Proxy API (temporarily disabled during Phase 2 refactor #221)
        .route("/api/proxy/status", get(handlers::proxy::status))
        .route("/api/proxy/start", post(handlers::proxy::start))
        .route("/api/proxy/stop", post(handlers::proxy::stop))
        // Hugging Face API
        .route(hf_routes::SEARCH_PATH, post(handlers::hf::search))
        .route(
            &format!("{}/:model_id", hf_routes::QUANTIZATIONS_PATH),
            get(handlers::hf::quantizations),
        )
        .route(
            &format!("{}/:model_id", hf_routes::TOOL_SUPPORT_PATH),
            get(handlers::hf::tool_support),
        )
        // Events (SSE)
        .route("/api/events", get(handlers::events::stream))
        // Bind AppState to non-chat routes
        .with_state(state.clone());

    // Build chat routes with minimal ChatState (already state-applied)
    let chat_state = Arc::new(ChatApiContext {
        core: state.core.clone(),
        gui: state.gui.clone(),
    });
    let chat = chat_routes(chat_state);

    // Merge both routers (now both are state-applied) and add CORS
    app_routes.merge(chat).layer(cors)
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
async fn health_check() -> &'static str {
    "OK"
}
