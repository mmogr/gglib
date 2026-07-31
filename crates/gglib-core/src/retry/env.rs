//! Operator overrides for the retry budget.
//!
//! Follows the workspace's `GGLIB_*` escape-hatch convention: the defaults are
//! what almost everyone should run, and these exist for the operator who has a
//! reason to differ. Deliberately no settings-table entry — a resilience budget
//! is not a user preference, and a migration for it would be unearned.

use std::sync::OnceLock;
use std::time::Duration;

use super::policy::RetryPolicy;

/// Overrides how many attempts a retry sequence may make, including the first.
pub const MAX_ATTEMPTS_ENV_VAR: &str = "GGLIB_LLM_RETRY_MAX_ATTEMPTS";

/// Overrides the wall-clock ceiling on a whole retry sequence, in seconds.
pub const DEADLINE_ENV_VAR: &str = "GGLIB_LLM_RETRY_DEADLINE_SECS";

impl RetryPolicy {
    /// [`RetryPolicy::default`] with any environment overrides applied.
    ///
    /// Resolved once per process: these are operator settings, and re-reading
    /// them per request would cost a syscall on every completion for a value
    /// that cannot meaningfully change mid-run.
    ///
    /// An unset or unparseable variable leaves that field at its default —
    /// a typo degrades to standard behaviour rather than disabling retry.
    #[must_use]
    pub fn from_env() -> Self {
        static RESOLVED: OnceLock<RetryPolicy> = OnceLock::new();
        *RESOLVED.get_or_init(|| Self::default().with_env_overrides(&std::env::var))
    }

    /// A policy that makes one attempt and never retries.
    ///
    /// What the CLI's `--no-retry` resolves to, and what a caller wanting
    /// strictly one-shot behaviour should ask for by name rather than by
    /// knowing that `max_attempts: 1` means "off".
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }

    /// Apply overrides read through `lookup`, which is the process environment
    /// in production and a fixture in tests.
    fn with_env_overrides<F, E>(mut self, lookup: &F) -> Self
    where
        F: Fn(&'static str) -> Result<String, E>,
    {
        if let Some(attempts) = parse::<u32, _, _>(lookup, MAX_ATTEMPTS_ENV_VAR) {
            // Zero attempts would mean never issuing the request at all, which
            // is nobody's intent; one is the "off" the operator meant.
            self.max_attempts = attempts.max(1);
        }
        if let Some(secs) = parse::<u64, _, _>(lookup, DEADLINE_ENV_VAR) {
            self.total_deadline = Duration::from_secs(secs);
        }

        // A single delay must never be able to consume the whole budget, or the
        // first backoff would overrun the deadline and silently disable
        // retrying. Halving guarantees at least one retry still fits inside a
        // shortened window.
        let ceiling = self.total_deadline / 2;
        if self.max_backoff > ceiling {
            self.max_backoff = ceiling;
        }

        self
    }
}

/// Read and parse one variable, warning on a value that cannot be used.
fn parse<T, F, E>(lookup: &F, name: &'static str) -> Option<T>
where
    T: std::str::FromStr,
    F: Fn(&'static str) -> Result<String, E>,
{
    let raw = lookup(name).ok()?;
    let parsed = raw.trim().parse::<T>().ok();
    if parsed.is_none() {
        tracing::warn!(value = %raw, "{name} is not a valid whole number — ignoring it");
    }
    parsed
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod env_tests;
