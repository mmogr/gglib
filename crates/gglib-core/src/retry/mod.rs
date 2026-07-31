#![doc = include_str!("README.md")]
mod env;
mod jitter;
mod policy;

pub use env::{DEADLINE_ENV_VAR, MAX_ATTEMPTS_ENV_VAR};
pub use jitter::jitter_unit;
pub use policy::{GiveUpReason, RetryDecision, RetryPolicy, decide};

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;
