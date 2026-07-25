//! CORS configuration types.
//!
//! Provides [`CorsConfig`] to control which origins are allowed
//! by the Axum web server's CORS middleware.

/// CORS configuration for the web server.
#[derive(Debug, Clone, Default)]
pub enum CorsConfig {
    /// Allow all origins (development mode).
    AllowAll,
    /// Allow specific origins (production mode).
    AllowOrigins(Vec<String>),
    /// Restrict to local-only access (localhost, 127.0.0.1, `::1`).
    #[default]
    LocalOnly,
}
