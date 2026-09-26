#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend — the #721 daemon wedge was this bug class. Denied here
// because neither crate inherits the workspace clippy lints yet.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

mod access;
mod admin;
#[allow(
    clippy::redundant_closure_for_method_calls,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod audit_records;
// Crate-internal. The seven that stay `pub` below are the ones other crates
// name by path: dashboard, loopback, models, props, repair, slot_eviction, slots.
// `server` is internal too — the root re-exports `serve`, which is all
// anyone needs from it.
// Without `test-support` the re-export below is absent, which is the point of
// gating it — but that also leaves `StreamConfig`, `restore_with_retry` and
// `LastLoadedSession` (a public field type of the first, so it rides along) with
// no public path, and this crate denies `unreachable_pub`. Said once here rather
// than at each of the three, and only for the configuration where it is true:
// with the feature on the lint applies normally, which is the configuration CI's
// `--all-features` clippy run checks.
#[cfg_attr(not(any(test, feature = "test-support")), allow(unreachable_pub))]
#[allow(
    clippy::redundant_clone,
    clippy::single_match_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod cache_lifecycle;
#[allow(
    clippy::format_collect,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod canonicalization;
#[allow(
    clippy::items_after_statements,
    clippy::missing_fields_in_debug,
    clippy::redundant_closure_for_method_calls,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod connections;
pub mod dashboard;
pub(crate) mod embeddings;
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::option_if_let_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod forward;
pub(crate) mod forward_unary;
pub(crate) mod load_endpoint;
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod loop_guard;
pub(crate) mod loop_guard_note;
pub(crate) mod loop_guard_step;
pub mod loopback;
pub(crate) mod mcp;
#[allow(
    clippy::redundant_closure_for_method_calls,
    clippy::struct_excessive_bools,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod metrics;
#[allow(
    clippy::match_same_arms,
    clippy::unreadable_literal,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod models;
pub(crate) mod models_endpoint;
pub(crate) mod observers;
pub(crate) mod profiles;
#[allow(
    clippy::match_wildcard_for_single_variants,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod props;
pub(crate) mod remote;
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod repair;
pub(crate) mod router;
#[allow(
    clippy::significant_drop_tightening,
    reason = "a guard's scope is its critical section, so this lint is never applied \
              in admission, residency or proxy lock code"
)]
#[allow(
    clippy::cast_sign_loss,
    clippy::large_types_passed_by_value,
    clippy::redundant_closure_for_method_calls,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod sampling_audit;
#[allow(
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod server;
pub mod slot_eviction;
#[allow(
    clippy::cast_precision_loss,
    clippy::manual_let_else,
    clippy::option_if_let_else,
    clippy::too_long_first_doc_paragraph,
    clippy::unreadable_literal,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod slots;
#[allow(
    clippy::single_match_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod slots_poller;
#[allow(
    clippy::needless_continue,
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod sse_stream;
pub mod template_caps_read;
#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::significant_drop_tightening,
    clippy::suboptimal_flops,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod token_calibration;
pub(crate) mod unary_body;

pub(crate) mod upstream_health;
pub(crate) mod upstream_read;

pub use observers::ProxyObservers;
pub use server::serve;
// Named by this crate's own `tests/`, which link it as an external crate and so
// cannot see `#[cfg(test)]`. Re-exported rather than reopening
// `cache_lifecycle`, and gated so the export exists for the test build only —
// nothing in the workspace wants these two, and a release build should not carry
// them. Mirrors `gglib-db`'s `test-utils`.
#[cfg(any(test, feature = "test-support"))]
pub use cache_lifecycle::{StreamConfig, restore_with_retry};
// Same gate, same reason: `tests/` start `serve` inside
// `TEST_STREAM_BOUNDS.scope(..)` so a stall takes milliseconds, not minutes.
#[cfg(any(test, feature = "test-support"))]
pub use upstream_read::{StreamBounds, TEST_STREAM_BOUNDS};
