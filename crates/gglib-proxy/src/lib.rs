#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend — the #721 daemon wedge was this bug class. Denied here
// because neither crate inherits the workspace clippy lints yet.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

mod access;
// Crate-internal. The six that stay `pub` below are the ones other crates
// name by path: dashboard, models, props, repair, slot_eviction, slots.
// `server` goes internal too — the root re-exports `serve`, which is all
// anyone wanted from it.
pub(crate) mod cache_lifecycle;
pub(crate) mod canonicalization;
pub(crate) mod connections;
pub mod dashboard;
pub(crate) mod embeddings;
pub(crate) mod forward;
pub(crate) mod loop_guard;
pub(crate) mod mcp;
pub(crate) mod metrics;
pub mod models;
pub(crate) mod profiles;
pub mod props;
pub mod repair;
pub(crate) mod sampling_audit;
pub(crate) mod server;
pub(crate) mod settings_cache;
pub mod slot_eviction;
pub mod slots;
pub(crate) mod slots_poller;
pub(crate) mod sse_stream;
pub(crate) mod token_calibration;

pub(crate) mod upstream_health;

pub use server::serve;
// Named by this crate's own `tests/`, which link it as an external crate.
// Re-exported rather than reopening `cache_lifecycle`, so the rest of that
// module stays auditable.
pub use cache_lifecycle::{StreamConfig, restore_with_retry};
