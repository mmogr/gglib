#![doc = include_str!("README.md")]
mod policy;

pub use policy::{GiveUpReason, RetryDecision, RetryPolicy, decide};

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;
