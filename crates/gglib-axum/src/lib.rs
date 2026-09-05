#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
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
use gglib_app_services as _;
use gglib_mcp as _;
use gglib_runtime as _;
use serde as _;
use serde_json as _;
use tokio as _;
use tokio_stream as _;
use tracing as _;
use tracing_subscriber as _; // Used by main.rs binary

// Crate-internal: the re-export list below is the whole public surface. Only
// `daemon` had consumers naming it by module path, and the three items they
// wanted are re-exported instead — which keeps the 35 `pub mod` under
// `handlers` auditable rather than reachable from anywhere in the workspace.
pub(crate) mod access;
pub(crate) mod bootstrap;
pub(crate) mod chat_api;
pub(crate) mod daemon;
pub(crate) mod dto;
pub(crate) mod error;
pub(crate) mod handlers;
pub(crate) mod proxy_watch;
pub(crate) mod routes;
pub(crate) mod sse;
pub(crate) mod state;
pub(crate) mod ui;

// Re-export primary types
pub use access::DaemonAccess;
pub use bootstrap::{AxumContext, ServerConfig, bootstrap, start_server};
pub use daemon::{DaemonLock, DaemonOptions, run_daemon};
pub use error::HttpError;
pub use gglib_core::CorsConfig;
pub use routes::{create_router, create_spa_router};
pub use state::AppState;
pub use ui::{create_embedded_spa_router, has_embedded_ui};
