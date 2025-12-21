//! `HuggingFace` client for searching models and fetching metadata.
//!
//! This module provides the main client interface for interacting with
//! the `HuggingFace` Hub API.

// Constructor is used via port.rs which compiler doesn't detect
#![allow(dead_code)]

mod repo_files;
mod search;

use crate::config::HfClientConfig;
use crate::http::{HttpBackend, ReqwestBackend};
use crate::models::HfConfig;
use url::Url;

// ============================================================================
// Type Aliases
// ============================================================================

/// Default `HuggingFace` client using the reqwest HTTP backend.
pub type DefaultHfClient = HfClient<ReqwestBackend>;

// ============================================================================
// Client
// ============================================================================

/// Client for interacting with the `HuggingFace` Hub API.
///
/// This client is generic over an HTTP backend, allowing for easy testing.
/// Use `DefaultHfClient` for production code. The generic parameter `B` is
/// an implementation detail - external code should not instantiate this
/// directly but use `DefaultHfClient::new()`.
pub struct HfClient<B: HttpBackend> {
    pub(crate) backend: B,
    pub(crate) config: HfConfig,
}

impl DefaultHfClient {
    /// Create a new client with the given configuration.
    pub fn new(config: &HfClientConfig) -> Self {
        let internal_config = Self::to_internal_config(config);
        let backend = ReqwestBackend::new(&internal_config);
        Self {
            backend,
            config: internal_config,
        }
    }

    /// Create a new client with default configuration.
    #[must_use]
    pub fn default_client() -> Self {
        Self::new(&HfClientConfig::default())
    }

    fn to_internal_config(config: &HfClientConfig) -> HfConfig {
        HfConfig {
            base_url: Url::parse(&config.base_url).unwrap_or_else(|_| {
                Url::parse("https://huggingface.co/api/models").expect("default URL is valid")
            }),
            token: config.token.clone(),
            max_retries: config.max_retries,
            #[allow(clippy::cast_possible_truncation)] // Duration milliseconds won't exceed u64 in practice
            retry_base_delay_ms: config.retry_base_delay.as_millis() as u64,
        }
    }
}

impl<B: HttpBackend> HfClient<B> {
    /// Create a new client with a custom backend.
    ///
    /// Use this for testing with a fake backend.
    #[cfg(test)]
    pub(crate) const fn with_backend(config: HfConfig, backend: B) -> Self {
        Self { backend, config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::{CannedResponse, FakeBackend};
    use serde_json::json;

    pub fn test_config() -> HfConfig {
        HfConfig::default()
    }

    pub fn fake_model_json(id: &str, downloads: u64) -> serde_json::Value {
        json!({
            "id": id,
            "downloads": downloads,
            "likes": 10,
            "siblings": [{"rfilename": "model.gguf"}]
        })
    }

    #[test]
    fn test_default_client_creation() {
        let config = HfClientConfig::new();
        let _client = DefaultHfClient::new(&config);
    }

    #[test]
    fn test_client_with_fake_backend() {
        let backend = FakeBackend::new().with_response(
            "test",
            CannedResponse {
                json: json!({"test": true}),
                has_more: false,
            },
        );
        let _client = HfClient::with_backend(test_config(), backend);
    }
}
