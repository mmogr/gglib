//! Embedded Axum server for Tauri desktop app.
//!
//! This module provides a minimal, secure HTTP API server that can be embedded
//! in Tauri applications. The server binds to an ephemeral port on localhost
//! and requires Bearer token authentication for all `/api/*` endpoints.
//!
//! # Security Model
//!
//! - **Ephemeral port**: Binds to `127.0.0.1:0` (OS chooses available port)
//! - **Bearer token auth**: Random UUID token required for all `/api/*` requests
//! - **Localhost only**: Server only accessible from `127.0.0.1`
//! - **Strict CORS**: Only allows requests from Tauri WebView origins
//!
//! # Usage
//!
//! ```ignore
//! use gglib_axum::embedded::{EmbeddedServerConfig, start_embedded_server};
//!
//! let config = EmbeddedServerConfig {
//!     cors_origins: vec![
//!         "tauri://localhost".to_string(),
//!         "http://tauri.localhost".to_string(),
//!     ],
//! };
//!
//! let (info, handle) = start_embedded_server(ctx, config).await?;
//! println!("API available at http://127.0.0.1:{}", info.port);
//! println!("Token: {}", info.token);
//! ```
//!
//! # Manual Verification (for development)
//!
//! To test the embedded server manually during development:
//!
//! 1. Open Tauri DevTools console and run:
//!    ```js
//!    const { port, token } = await window.__TAURI__.invoke('get_embedded_api_info');
//!    console.log(`Port: ${port}, Token: ${token}`);
//!    ```
//!
//! 2. Use curl to verify endpoints:
//!    ```bash
//!    # Health check (no auth required)
//!    curl http://127.0.0.1:$PORT/health
//!
//!    # API endpoint without auth (should fail with 401)
//!    curl -i http://127.0.0.1:$PORT/api/models
//!
//!    # API endpoint with correct token (should succeed)
//!    curl -i -H "Authorization: Bearer $TOKEN" http://127.0.0.1:$PORT/api/models
//!    ```
//!
//! Note: The token is only logged in debug builds when `GGLIB_LOG_EMBEDDED_TOKEN=1`
//! is set. In production, use the Tauri command to retrieve it.
//!
//! # Manual Verification (for development)
//!
//! To test the embedded server manually during development:
//!
//! 1. Open Tauri DevTools console and run:
//!    ```js
//!    const { port, token } = await window.__TAURI__.invoke('get_embedded_api_info');
//!    console.log(`Port: ${port}, Token: ${token}`);
//!    ```
//!
//! 2. Use curl to verify endpoints:
//!    ```bash
//!    # Health check (no auth required)
//!    curl http://127.0.0.1:$PORT/health
//!
//!    # API endpoint without auth (should fail with 401)
//!    curl -i http://127.0.0.1:$PORT/api/models
//!
//!    # API endpoint with correct token (should succeed)
//!    curl -i -H "Authorization: Bearer $TOKEN" http://127.0.0.1:$PORT/api/models
//!    ```
//!
//! Note: The token is only logged in debug builds when `GGLIB_LOG_EMBEDDED_TOKEN=1`
//! is set. In production, use the Tauri command to retrieve it.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::Request,
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use tokio::task::JoinHandle;
use tower_http::cors::CorsLayer;

use crate::{
    bootstrap::AxumContext,
    routes::{api_routes, health_check},
    state::AppState,
};

/// Information about the running embedded API server.
///
/// Returned by [`start_embedded_server`] for the frontend to discover
/// the server's port and authentication token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddedApiInfo {
    /// The ephemeral port the server is listening on.
    pub port: u16,
    /// The Bearer token required for API authentication.
    pub token: String,
}

/// Configuration for the embedded API server.
#[derive(Debug, Clone)]
pub struct EmbeddedServerConfig {
    /// CORS allowed origins (e.g., "tauri://localhost", "http://localhost:5173")
    pub cors_origins: Vec<String>,
}

/// Default CORS origins for embedded server.
///
/// Returns the standard set of origins that should be allowed:
/// - Tauri WebView protocols (tauri:// and http(s)://tauri.localhost)
/// - Local development server (Vite default port 5173)
///
/// Use this to avoid duplicating origin lists between Tauri and web configs.
///
/// # Example
///
/// ```ignore
/// let config = EmbeddedServerConfig {
///     cors_origins: gglib_axum::embedded::default_embedded_cors_origins(),
/// };
/// ```
pub fn default_embedded_cors_origins() -> Vec<String> {
    vec![
        "tauri://localhost".into(),
        "http://tauri.localhost".into(),
        "https://tauri.localhost".into(),
        "http://localhost:5173".into(), // Vite dev server
    ]
}

/// Start an embedded Axum API server for Tauri.
///
/// Returns `(EmbeddedApiInfo, JoinHandle)` on success. The join handle can be
/// used to await server shutdown (though typically servers run for application lifetime).
///
/// # Security
///
/// - Binds to `127.0.0.1:0` (ephemeral port, localhost only)
/// - Generates a random UUID token for Bearer authentication
/// - All `/api/*` endpoints require `Authorization: Bearer {token}` header
/// - `/health` endpoint is unauthenticated
/// - CORS restricted to `cors_origins` only
///
/// # Example
///
/// ```ignore
/// let config = EmbeddedServerConfig {
///     cors_origins: vec!["tauri://localhost".to_string()],
/// };
/// let (info, _handle) = start_embedded_server(ctx, config).await?;
/// ```
pub async fn start_embedded_server(
    ctx: AxumContext,
    cfg: EmbeddedServerConfig,
) -> Result<(EmbeddedApiInfo, JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    let state: AppState = Arc::new(ctx);

    // Generate token and store full "Bearer <token>" for middleware
    // This avoids per-request format!() allocation in the auth check
    let token: String = uuid::Uuid::new_v4().to_string();
    let expected_header: Arc<str> = Arc::from(format!("Bearer {}", token));

    // Log token info based on build profile
    #[cfg(debug_assertions)]
    {
        if std::env::var("GGLIB_LOG_EMBEDDED_TOKEN").is_ok() {
            tracing::info!(token = %token, "Generated API token (debug mode)");
        } else {
            tracing::info!(
                token_prefix = &token[..8],
                "Generated API token (set GGLIB_LOG_EMBEDDED_TOKEN=1 to see full token)"
            );
        }
    }
    #[cfg(not(debug_assertions))]
    {
        tracing::info!("Generated API token");
    }

    let cors = build_cors(&cfg.cors_origins)?;

    let auth_layer = middleware::from_fn(move |req: Request, next: Next| {
        let expected = expected_header.clone();
        async move { validate_bearer(expected, req, next).await }
    });

    // IMPORTANT: apply auth + CORS only on /api
    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/api", api_routes().route_layer(auth_layer).layer(cors))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
    let port = listener.local_addr()?.port();

    tracing::info!(
        port = port,
        auth_enabled = true,
        "Starting embedded API server"
    );

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "Embedded API server error");
        }
    });

    Ok((EmbeddedApiInfo { port, token }, handle))
}

/// Auth middleware: validate Bearer token.
///
/// Requires `Authorization: Bearer {token}` header.
/// Returns 401 Unauthorized with `WWW-Authenticate: Bearer` on failure.
///
/// # Performance
///
/// The `expected` parameter contains the full "Bearer <token>" string,
/// so we can do a direct string comparison without allocating.
async fn validate_bearer(
    expected: Arc<str>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth {
        Some(h) if h == expected.as_ref() => Ok(next.run(req).await),
        _ => {
            tracing::warn!(
                path = %req.uri().path(),
                "Unauthorized API request - missing or invalid token"
            );
            let mut res = Response::new(axum::body::Body::empty());
            *res.status_mut() = StatusCode::UNAUTHORIZED;
            res.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
            Ok(res)
        }
    }
}

/// Build CORS layer for embedded server.
///
/// Allows specific origins, all standard methods (including OPTIONS for preflight),
/// and necessary headers for Bearer authentication.
fn build_cors(origins: &[String]) -> Result<CorsLayer, axum::http::header::InvalidHeaderValue> {
    let origins = origins
        .iter()
        .map(|o| o.parse())
        .collect::<Result<Vec<axum::http::HeaderValue>, _>>()?;

    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_api_info_serialization() {
        let info = EmbeddedApiInfo {
            port: 12345,
            token: "test-token-123".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("test-token-123"));

        let deserialized: EmbeddedApiInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.port, 12345);
        assert_eq!(deserialized.token, "test-token-123");
    }
}
