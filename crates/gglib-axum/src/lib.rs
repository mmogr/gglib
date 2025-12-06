//! Axum web server adapter for gglib.
//!
//! This crate provides an HTTP API for gglib using Axum. It is a library
//! crate only - a separate binary crate can be created later if a standalone
//! server is needed.
//!
//! # Architecture
//!
//! - `bootstrap` - Server startup (composition root)
//! - `error` - HTTP error types with status code mapping
//! - `routes` - Route definitions and router construction (stub - delegates to legacy)
//! - `handlers/` - Request handlers (to be added as migrated)
//!
//! # Usage
//!
//! ```rust,ignore
//! use gglib_axum::bootstrap;
//!
//! // Start the web server
//! bootstrap::start_server(9887, 9000, 5).await?;
//! ```

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tokio_test as _;

// Dependencies used by bootstrap module
use anyhow as _;
use gglib as _;
use gglib_db as _;
use gglib_runtime as _;
use serde_json as _;
use tokio as _;
use tracing as _;

pub mod bootstrap;
pub mod error;
pub mod routes;

// Re-export primary types
pub use bootstrap::{AxumContext, ServerConfig, bootstrap, start_server};
pub use error::HttpError;
pub use routes::create_router;
