#![doc = include_str!("README.md")]
mod wait;

pub use wait::{DEFAULT_WAIT, WAIT_ENV_VAR, ensure_with_contention_wait, wait_from_env};
