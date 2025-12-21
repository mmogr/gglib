//! Public configuration for the `HuggingFace` client.
//!
//! This module provides a stable public API for configuring the HF client.
//! The internal config is derived from this.

use std::time::Duration;

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
///     .with_user_agent("my-app/1.0");
/// ```
#[derive(Debug, Clone)]
pub struct HfClientConfig {
    /// Base URL for the `HuggingFace` API
    pub(crate) base_url: String,
    /// User agent string for HTTP requests
    pub(crate) user_agent: String,
    /// Request timeout
    pub(crate) timeout: Duration,
    /// Optional authentication token for private models
    pub(crate) token: Option<String>,
    /// Maximum number of retry attempts for transient errors
    pub(crate) max_retries: u8,
    /// Base delay for exponential backoff
    pub(crate) retry_base_delay: Duration,
}

impl Default for HfClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://huggingface.co/api/models".to_string(),
            user_agent: concat!("gglib-hf/", env!("CARGO_PKG_VERSION")).to_string(),
            timeout: Duration::from_secs(30),
            token: None,
            max_retries: 3,
            retry_base_delay: Duration::from_millis(500),
        }
    }
}

impl HfClientConfig {
    /// Create a new configuration with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base URL for the `HuggingFace` API.
    ///
    /// Defaults to `https://huggingface.co/api/models`.
    #[must_use]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the user agent string for HTTP requests.
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
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

    /// Set the maximum number of retry attempts for transient errors.
    ///
    /// Defaults to 3 retries.
    #[must_use]
    pub const fn with_max_retries(mut self, retries: u8) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the base delay for exponential backoff retries.
    ///
    /// Defaults to 500ms.
    #[must_use]
    pub const fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_base_delay = delay;
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
        assert!(config.user_agent.contains("gglib-hf"));
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.token.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_builder_pattern() {
        let config = HfClientConfig::new()
            .with_base_url("https://custom.api/")
            .with_user_agent("test-agent")
            .with_timeout(Duration::from_secs(60))
            .with_token("secret")
            .with_max_retries(5);

        assert_eq!(config.base_url, "https://custom.api/");
        assert_eq!(config.user_agent, "test-agent");
        assert_eq!(config.timeout, Duration::from_secs(60));
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
}
