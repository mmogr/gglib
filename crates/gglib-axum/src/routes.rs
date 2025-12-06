//! Route definitions and router construction.
//!
//! This module defines the HTTP routes and creates the main router.

use axum::routing::get;
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Application state shared across handlers.
///
/// This will be expanded as handlers are migrated.
pub struct AppState<R> {
    /// Model repository for database operations.
    pub model_repo: R,
}

/// Create the main Axum router with all API routes.
///
/// # Type Parameters
///
/// * `R` - Model repository implementing `ModelRepository + Send + Sync + 'static`
///
/// # Arguments
///
/// * `state` - The application state to share with handlers
///
/// # Returns
///
/// An Axum Router ready to be served with `axum::serve()`.
pub fn create_router<R>(state: Arc<AppState<R>>) -> Router
where
    R: Send + Sync + 'static,
{
    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Health check endpoint
        .route("/health", get(health_check))
        // API routes will be added as handlers are migrated:
        // .route("/api/models", get(handlers::list_models))
        // .route("/api/models/:id", get(handlers::get_model))
        // etc.
        .layer(cors)
        .with_state(state)
}

/// Health check endpoint.
///
/// Returns 200 OK with a simple status message.
async fn health_check() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRepo;

    #[test]
    fn test_router_builds() {
        let state = Arc::new(AppState { model_repo: MockRepo });
        let _router = create_router(state);
        // If we get here, the router was successfully built
    }
}
