//! Public configuration for the `HuggingFace` client.
//!
//! This module provides a stable public API for configuring the HF client.
//! The internal config is derived from this.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::hub_limit::{HubGate, OnLimit};

/// Configuration for the `HuggingFace` client.
///
/// Use the builder pattern methods to customize the client configuration.
///
/// # Example
///
/// ```
/// use gglib_hf::HfClientConfig;
/// use std::time::Duration;
///
/// let config = HfClientConfig::new()
///     .with_timeout(Duration::from_secs(60))
///     .with_token("hf_...");
/// ```
#[derive(Debug, Clone)]
pub struct HfClientConfig {
    /// Base URL for the `HuggingFace` API
    pub(crate) base_url: String,
    /// Request timeout
    pub(crate) timeout: Duration,
    /// Optional authentication token for private models
    pub(crate) token: Option<String>,
    /// Maximum number of retry attempts for transient errors
    pub(crate) max_retries: u8,
    /// Base delay for exponential backoff
    pub(crate) retry_base_delay: Duration,
    /// The gate the client keeps the Hub's rate limit in, when it shares
    /// one; with `None` it keeps its own.
    pub(crate) gate: Option<Arc<HubGate>>,
    /// What the client does at the Hub's rate limit.
    pub(crate) on_limit: OnLimit,
}

impl Default for HfClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://huggingface.co/api/models".to_string(),
            timeout: Duration::from_secs(30),
            token: None,
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
            gate: None,
            on_limit: OnLimit::Surface,
        }
    }
}

impl HfClientConfig {
    /// Create a new configuration with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the request timeout.
    ///
    /// Defaults to 30 seconds.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set an authentication token for accessing private models.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set an optional authentication token.
    #[must_use]
    pub fn with_optional_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Whether requests made with this configuration carry a token.
    #[must_use]
    pub const fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// Keep the Hub's rate limit in `gate`, shared with whatever else asks
    /// the Hub from this address, rather than in a gate of the client's own.
    #[must_use]
    pub fn with_gate(mut self, gate: Arc<HubGate>) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Meet the Hub's rate limit as `on_limit` says.
    ///
    /// Defaults to [`OnLimit::Surface`]: a refused request, and one asked of
    /// an allowance known to be spent, fail at once with the time the limit
    /// lifts.
    #[must_use]
    pub fn with_on_limit(mut self, on_limit: OnLimit) -> Self {
        self.on_limit = on_limit;
        self
    }

    /// Set the maximum number of retry attempts for transient errors.
    ///
    /// Defaults to 3 retries.
    #[must_use]
    pub const fn with_max_retries(mut self, retries: u8) -> Self {
        self.max_retries = retries;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HfClientConfig::new();
        assert_eq!(config.base_url, "https://huggingface.co/api/models");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.token.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_builder_pattern() {
        let config = HfClientConfig::new()
            .with_timeout(Duration::from_mins(1))
            .with_token("secret")
            .with_max_retries(5);

        assert_eq!(config.timeout, Duration::from_mins(1));
        assert_eq!(config.token, Some("secret".to_string()));
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_optional_token() {
        let with_token = HfClientConfig::new().with_optional_token(Some("token".to_string()));
        assert_eq!(with_token.token, Some("token".to_string()));

        let without_token = HfClientConfig::new().with_optional_token(None);
        assert!(without_token.token.is_none());
    }

    #[test]
    fn a_config_says_whether_it_holds_a_token() {
        assert!(HfClientConfig::new().with_token("token").has_token());
        assert!(!HfClientConfig::new().has_token());
    }
}
