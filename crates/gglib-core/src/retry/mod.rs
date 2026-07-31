#![doc = include_str!("README.md")]
mod jitter;
mod policy;

pub use jitter::jitter_unit;
pub use policy::{GiveUpReason, RetryDecision, RetryPolicy, decide};

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;
