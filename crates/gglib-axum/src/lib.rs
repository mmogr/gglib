#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

// Crate-internal: the re-export list below is the whole public surface,
// and nothing under `handlers` is reachable from outside this crate.
pub(crate) mod access;
pub(crate) mod bootstrap;
#[allow(
    clippy::option_option,
    clippy::ref_option,
    clippy::struct_field_names,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod chat_api;
pub(crate) mod config;
pub(crate) mod daemon;
pub(crate) mod dto;
pub(crate) mod error;
pub(crate) mod handlers;
pub(crate) mod proxy_watch;
pub(crate) mod routes;
pub(crate) mod routes_benchmark;
pub(crate) mod routes_remote;
pub(crate) mod routes_runs;
pub(crate) mod sse;
pub(crate) mod state;
pub(crate) mod trust;
pub(crate) mod ui;

// Re-export primary types
pub use access::DaemonAccess;
pub use bootstrap::{AxumContext, bootstrap, start_server};
pub use config::ServerConfig;
pub use daemon::{DaemonLock, DaemonOptions, run_daemon};
pub use error::HttpError;
pub use gglib_core::CorsConfig;
pub use routes::{create_router, create_spa_router};
pub use state::AppState;
pub use ui::{create_embedded_spa_router, has_embedded_ui};
