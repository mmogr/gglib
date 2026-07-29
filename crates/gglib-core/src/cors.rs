//! CORS configuration types.
//!
//! Provides [`CorsConfig`] to control which origins are allowed
//! by the Axum web server's CORS middleware.

/// CORS configuration for the web server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CorsConfig {
    /// Allow all origins (development mode).
    AllowAll,
    /// Allow specific origins (production mode).
    AllowOrigins(Vec<String>),
    /// Restrict to local-only access.
    ///
    /// Accepts `localhost`, `127.0.0.1`, `::1`, `tauri.localhost`,
    /// and Tauri custom schemes (`tauri://localhost`, `asset://localhost`).
    /// This is the default for both the web server and the proxy.
    #[default]
    LocalOnly,
}
