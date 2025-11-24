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
        .route("/api/models/:id", get(handlers::get_model))
        .route("/api/models/:id", put(handlers::update_model))
        .route("/api/models/:id", delete(handlers::remove_model))
        // Server management routes
        .route("/api/models/:id/start", post(handlers::start_server))
        .route("/api/models/:id/stop", post(handlers::stop_server))
        .route("/api/servers", get(handlers::list_servers))
        // Proxy routes
        .route("/api/proxy/status", get(handlers::get_proxy_status))
        .route("/api/proxy/start", post(handlers::start_proxy))
        .route("/api/proxy/stop", post(handlers::stop_proxy))
        // Chat proxy
        .route("/api/chat", post(handlers::chat_proxy))
        // Tag routes
        .route("/api/tags", get(handlers::list_tags))
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
