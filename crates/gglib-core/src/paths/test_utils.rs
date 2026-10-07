//! Test utilities for tests that need environment variable isolation.
//!
//! This module provides synchronization primitives to prevent race conditions
//! in tests that modify environment variables like `GGLIB_DATA_DIR`. Most of
//! those are path tests, which is why it lives here; the rest of the crate's
//! tests may use it too.

use std::env;
use std::sync::Mutex;

/// Shared lock to serialize tests that depend on environment variables.
///
/// All tests that read or write environment variables (especially `GGLIB_DATA_DIR`)
/// must acquire this lock to prevent race conditions. Without this, concurrent
/// tests can interfere with each other's environment state.
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard that restores an environment variable to its original value on drop.
///
/// # Example
///
/// ```ignore
/// let _guard = ENV_LOCK.lock().unwrap();
/// let _env = EnvVarGuard::set("GGLIB_DATA_DIR", "/tmp/test");
/// // ... test code that uses GGLIB_DATA_DIR ...
/// // Original value restored when _env is dropped
/// ```
pub(crate) struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    /// Set an environment variable and return a guard that will restore it.
    #[allow(unsafe_code)]
    pub(crate) fn set(key: &str, value: &str) -> Self {
        let previous = env::var(key).ok();
        unsafe {
            env::set_var(key, value);
        }
        Self {
            key: key.to_string(),
            previous,
        }
    }

    /// Remove an environment variable and return a guard that will restore it.
    #[allow(
        unsafe_code,
        reason = "a test changes its own environment, under ENV_LOCK"
    )]
    pub(crate) fn unset(key: &str) -> Self {
        let previous = env::var(key).ok();
        unsafe {
            env::remove_var(key);
        }
        Self {
            key: key.to_string(),
            previous,
        }
    }
}

impl Drop for EnvVarGuard {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if let Some(ref value) = self.previous {
            unsafe {
                env::set_var(&self.key, value);
            }
        } else {
            unsafe {
                env::remove_var(&self.key);
            }
        }
    }
}
