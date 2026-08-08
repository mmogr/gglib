#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend — the #721 daemon wedge was this bug class. Denied here
// because neither crate inherits the workspace clippy lints yet.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

mod access;
pub mod cache_lifecycle;
pub mod canonicalization;
pub mod connections;
pub mod dashboard;
pub mod embeddings;
pub mod forward;
pub mod loop_guard;
pub mod mcp;
pub mod metrics;
pub mod models;
pub mod profiles;
pub mod repair;
pub mod server;
pub mod settings_cache;
pub mod slot_eviction;
pub mod slots;
pub mod slots_poller;
pub mod sse_stream;
pub mod token_calibration;

pub mod upstream_health;

pub use server::serve;
