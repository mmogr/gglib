//! API routes configuration.
//!
//! This module defines all HTTP routes and their mappings to handlers.

use crate::commands::gui_web::{handlers, state::AppState};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Build the API router with all routes
pub fn api_routes(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // Settings routes
        .route(
            "/api/settings/models-directory",
            get(handlers::get_models_directory),
        )
        .route(
            "/api/settings/models-directory",
            put(handlers::update_models_directory),
        )
        // Application settings routes
        .route("/api/settings", get(handlers::get_settings))
        .route("/api/settings", put(handlers::update_settings))
        // System info routes
        .route("/api/system/memory", get(handlers::get_system_memory_info))
        // Model routes
        .route("/api/models", get(handlers::list_models))
        .route("/api/models", post(handlers::add_model))
        // Specific routes MUST come before wildcard :id routes
        .route("/api/models/download", post(handlers::download_model))
        .route(
            "/api/models/download/cancel",
            post(handlers::cancel_download),
        )
        .route(
            "/api/models/download/progress",
            get(handlers::stream_progress),
        )
        // Download queue routes
        .route(
            "/api/models/download/queue",
            get(handlers::get_download_queue),
        )
        .route("/api/models/download/queue", post(handlers::queue_download))
        .route(
            "/api/models/download/queue/remove",
            post(handlers::remove_from_download_queue),
        )
        .route(
            "/api/models/download/queue/reorder",
            post(handlers::reorder_download_queue),
        )
        .route(
            "/api/models/download/queue/clear-failed",
            post(handlers::clear_failed_downloads),
        )
        // HuggingFace browser routes
        .route("/api/hf/browse", post(handlers::browse_hf_models))
        .route(
            "/api/hf/quantizations/:model_id",
            get(handlers::get_hf_quantizations),
        )
        .route("/api/models/:id", get(handlers::get_model))
        .route("/api/models/:id", put(handlers::update_model))
        .route("/api/models/:id", delete(handlers::remove_model))
        // Server management routes
        .route("/api/models/:id/start", post(handlers::start_server))
        .route("/api/models/:id/stop", post(handlers::stop_server))
        .route("/api/servers", get(handlers::list_servers))
        // Server log routes
        .route("/api/servers/:port/logs", get(handlers::get_server_logs))
        .route(
            "/api/servers/:port/logs/stream",
            get(handlers::stream_server_logs),
        )
        .route(
            "/api/servers/:port/logs",
            delete(handlers::clear_server_logs),
        )
        // Proxy routes
        .route("/api/proxy/status", get(handlers::get_proxy_status))
        .route("/api/proxy/start", post(handlers::start_proxy))
        .route("/api/proxy/stop", post(handlers::stop_proxy))
        // Chat proxy
        .route("/api/chat", post(handlers::chat_proxy))
        // Tag routes
        .route("/api/tags", get(handlers::list_tags))
        // Model filter options route
        .route(
            "/api/models/filter-options",
            get(handlers::get_model_filter_options),
        )
        .route("/api/models/:model_id/tags", get(handlers::get_model_tags))
        .route("/api/models/:model_id/tags", post(handlers::add_model_tag))
        .route(
            "/api/models/:model_id/tags",
            delete(handlers::remove_model_tag),
        )
        // Chat history routes
        .route("/api/conversations", get(handlers::list_conversations))
        .route("/api/conversations", post(handlers::create_conversation))
        .route(
            "/api/conversations/:conversation_id",
            get(handlers::get_conversation),
        )
        .route(
            "/api/conversations/:conversation_id",
            put(handlers::update_conversation),
        )
        .route(
            "/api/conversations/:conversation_id",
            delete(handlers::delete_conversation),
        )
        .route(
            "/api/conversations/:conversation_id/messages",
            get(handlers::get_messages),
        )
        .route("/api/messages", post(handlers::save_message))
        .route("/api/messages/:message_id", put(handlers::update_message))
        .route(
            "/api/messages/:message_id",
            delete(handlers::delete_message),
        )
        // Inject state
        .with_state(state)
        // Enable CORS for local development
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}
