//! What the Axum adapter is given: [`ServerConfig`].
//!
//! Split from `bootstrap.rs`, the composition root, along the line
//! `gglib-bootstrap` draws between its own `config.rs` and `builder.rs`: this
//! is a value a caller builds and hands over, and `bootstrap` is what is done
//! with it.

use std::path::PathBuf;

use anyhow::Result;
use gglib_core::CorsConfig;
use gglib_core::paths::llama_server_path;

/// Server configuration for the Axum adapter.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host to bind the HTTP server.
    pub host: String,
    /// Port for the HTTP server.
    pub port: u16,
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent agent loop sessions.
    ///
    /// Each `POST /api/agent/chat` request holds one permit for the lifetime
    /// of its SSE stream.  When all permits are taken, new requests receive
    /// `429 Too Many Requests` immediately rather than queuing.
    pub max_concurrent_agent_loops: usize,
    /// Optional path to static assets for SPA serving.
    pub static_dir: Option<PathBuf>,
    /// CORS configuration.
    pub cors: CorsConfig,
    /// Database file to open. `None` resolves through [`database_path`](gglib_core::paths::database_path).
    ///
    /// Naming a path lets a caller run against a database of its own, which
    /// is what keeps the integration tests off the developer's: in a debug
    /// build [`database_path`](gglib_core::paths::database_path) resolves into the checkout itself. Either way
    /// the database layer creates the parent directory and makes it `0700`.
    pub db_path: Option<PathBuf>,
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            host: "127.0.0.1".into(),
            port: 9887,
            base_port: 9000,
            llama_server_path: llama_server_path()?,
            max_concurrent_agent_loops: 4,
            static_dir: None,
            cors: CorsConfig::default(),
            db_path: None,
        })
    }
}
