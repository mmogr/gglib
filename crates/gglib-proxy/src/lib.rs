#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend — the #721 daemon wedge was this bug class. Denied here
// because neither crate inherits the workspace clippy lints yet.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

mod access;
pub(crate) mod audit_records;
// Crate-internal. The six that stay `pub` below are the ones other crates
// name by path: dashboard, models, props, repair, slot_eviction, slots.
// `server` goes internal too — the root re-exports `serve`, which is all
// anyone wanted from it.
// Without `test-support` the re-export below is absent, which is the point of
// gating it — but that also leaves `StreamConfig`, `restore_with_retry` and
// `LastLoadedSession` (a public field type of the first, so it rides along) with
// no public path, and this crate denies `unreachable_pub`. Said once here rather
// than at each of the three, and only for the configuration where it is true:
// with the feature on the lint applies normally, which is the configuration CI's
// `--all-features` clippy run checks.
#[cfg_attr(not(any(test, feature = "test-support")), allow(unreachable_pub))]
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
pub(crate) mod models_endpoint;
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
pub mod template_caps_read;
pub(crate) mod token_calibration;

pub(crate) mod upstream_health;

pub use server::serve;
// Named by this crate's own `tests/`, which link it as an external crate and so
// cannot see `#[cfg(test)]`. Re-exported rather than reopening
// `cache_lifecycle`, and gated so the export exists for the test build only —
// nothing in the workspace wants these two, and a release build should not carry
// them. Mirrors `gglib-db`'s `test-utils`.
#[cfg(any(test, feature = "test-support"))]
pub use cache_lifecycle::{StreamConfig, restore_with_retry};
