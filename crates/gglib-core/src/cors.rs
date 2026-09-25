//! CORS configuration types.
//!
//! Provides [`CorsConfig`] to control which origins are allowed
//! by the Axum web server's CORS middleware.

use crate::is_local_origin;

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

impl CorsConfig {
    /// Whether a CORS layer built from this config lets a page served from
    /// `origin` read its answers.
    ///
    /// Both routers' CORS layers let an origin read exactly when this does,
    /// and [`may_change`](crate::access::may_change) asks it before it lets
    /// any page but the endpoint's own change anything, so a page that names
    /// any origin but the endpoint's own may change something exactly when
    /// the CORS layer lets it read the answer.
    /// `AllowOrigins` matches the origin exactly, as a browser serializes it.
    #[must_use]
    pub fn allows_origin(&self, origin: &str) -> bool {
        match self {
            Self::AllowAll => true,
            Self::AllowOrigins(origins) => origins.iter().any(|allowed| allowed == origin),
            Self::LocalOnly => is_local_origin(origin),
        }
    }
}
