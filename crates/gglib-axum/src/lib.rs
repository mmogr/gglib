#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use http_body_util as _;
#[cfg(test)]
use hyper as _;
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;
#[cfg(test)]
use tower as _;

// Dependencies used by bootstrap module
use anyhow as _;
use chrono as _;
use futures_util as _;
use gglib_db as _;
use gglib_gui as _;
use gglib_mcp as _;
use gglib_runtime as _;
use serde as _;
use serde_json as _;
use tokio as _;
use tokio_stream as _;
use tracing as _;
use tracing_subscriber as _; // Used by main.rs binary
use uuid as _; // Will be used by embedded module

pub mod bootstrap;
pub mod chat_api;
pub mod dto;
pub mod embedded;
pub mod error;
pub mod handlers;
pub mod routes;
pub mod sse;
pub mod state;

// Re-export primary types
pub use bootstrap::{AxumContext, CorsConfig, ServerConfig, bootstrap, start_server};
pub use embedded::{EmbeddedApiInfo, EmbeddedServerConfig, start_embedded_server};
pub use error::HttpError;
pub use routes::{create_router, create_spa_router};
pub use state::AppState;
