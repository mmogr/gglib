//! Environment provider trait for testable path resolution.

use std::ffi::OsString;

/// Trait for accessing environment variables (injectable for testing).
pub trait EnvProvider {
    /// Get an environment variable.
    fn get(&self, key: &str) -> Option<OsString>;
}

/// Production environment provider that reads from the actual process environment.
pub struct SystemEnv;

impl EnvProvider for SystemEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

/// Test/mock environment provider with predefined variables.
#[cfg(test)]
#[derive(Default)]
pub struct MockEnv {
    vars: std::collections::HashMap<String, OsString>,
}

#[cfg(test)]
impl MockEnv {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<OsString>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
impl EnvProvider for MockEnv {
    fn get(&self, key: &str) -> Option<OsString> {
        self.vars.get(key).cloned()
    }
}
