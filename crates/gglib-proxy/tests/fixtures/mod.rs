//! Test fixtures shared by proxy integration tests.
#![allow(dead_code)]
pub(crate) mod access;
#[allow(
    clippy::significant_drop_tightening,
    reason = "a guard's scope is its critical section, so this lint is never applied \
              in admission, residency or proxy lock code"
)]
#[allow(
    clippy::default_trait_access,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod common;
pub(crate) mod loop_guard;
pub(crate) mod profile_harness;
pub(crate) mod profile_mocks;
pub(crate) mod recorder;
pub(crate) mod remote;
pub(crate) mod sse;
pub(crate) mod stall;
pub(crate) mod tunnel;
