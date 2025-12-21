//! HTTP backend abstraction for `HuggingFace` API.
//!
//! This module provides a trait-based HTTP backend that allows for
//! dependency injection and easy testing. The production implementation
//! uses reqwest with automatic retry logic for transient errors.

// Constructor used by client::mod but compiler doesn't track cross-module usage well
#![allow(dead_code)]

use crate::error::{HfError, HfResult};
use crate::models::HfConfig;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::time::Duration;
use url::Url;

// ============================================================================
// HTTP Backend Trait
// ============================================================================

/// Trait for HTTP backends that can fetch JSON from URLs.
///
/// This abstraction allows for dependency injection of HTTP clients,
/// making it easy to test code that depends on HTTP requests.
///
/// This is an implementation detail - external code should use the `HfClientPort` trait.
#[async_trait]
pub trait HttpBackend: Send + Sync {
    /// Fetch JSON from a URL and deserialize it.
    async fn get_json<T: DeserializeOwned + Send>(&self, url: &Url) -> HfResult<T>;

    /// Fetch JSON from a URL and return the raw response with pagination info.
    async fn get_json_paginated<T: DeserializeOwned + Send>(
        &self,
        url: &Url,
    ) -> HfResult<(T, bool)>;
}

// ============================================================================
// Reqwest Backend
// ============================================================================

/// Production HTTP backend using reqwest with retry logic.
///
/// Implements exponential backoff for transient server errors (5xx)
/// and network errors.
///
/// This is an implementation detail - external code should use `DefaultHfClient`
/// and interact with it through the `HfClientPort` trait.
pub struct ReqwestBackend {
    client: reqwest::Client,
    max_retries: u8,
    retry_base_delay_ms: u64,
    auth_token: Option<String>,
}

impl ReqwestBackend {
    /// Create a new reqwest backend with the given configuration.
    pub fn new(config: &HfConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to create HTTP client");

        Self {
            client,
            max_retries: config.max_retries,
            retry_base_delay_ms: config.retry_base_delay_ms,
            auth_token: config.token.clone(),
        }
    }

    /// Build a request with optional authentication.
    fn build_request(&self, url: &Url) -> reqwest::RequestBuilder {
        let mut request = self.client.get(url.as_str());
        if let Some(ref token) = self.auth_token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        request
    }

    /// Fetch a URL with automatic retry for transient errors.
    async fn fetch_with_retry(&self, url: &Url) -> HfResult<reqwest::Response> {
        let mut last_error: Option<HfError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_millis(
                    self.retry_base_delay_ms * 2u64.pow(u32::from(attempt) - 1),
                );
                tokio::time::sleep(delay).await;
            }

            match self.build_request(url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }

                    // 5xx errors are retryable (server-side issues)
                    if status.is_server_error() && attempt < self.max_retries {
                        last_error = Some(HfError::ApiRequestFailed {
                            status: status.as_u16(),
                            url: url.to_string(),
                        });
                        continue;
                    }

                    // 404 is a special case
                    if status.as_u16() == 404 {
                        if let Some(model_id) = extract_model_id_from_path(url.path()) {
                            return Err(HfError::ModelNotFound { model_id });
                        }
                    }

                    // 4xx errors or final attempt - fail immediately
                    return Err(HfError::ApiRequestFailed {
                        status: status.as_u16(),
                        url: url.to_string(),
                    });
                }
                Err(e) => {
                    // Network errors are retryable
                    if attempt < self.max_retries {
                        last_error = Some(e.into());
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| HfError::InvalidResponse {
            message: "Unknown error during fetch".to_string(),
        }))
    }
}

/// Try to extract a model ID from an API path.
fn extract_model_id_from_path(path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    if let Some(rest) = path.strip_prefix("api/models/") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return Some(format!("{}/{}", parts[0], parts[1]));
        }
    }
    None
}

#[async_trait]
impl HttpBackend for ReqwestBackend {
    async fn get_json<T: DeserializeOwned + Send>(&self, url: &Url) -> HfResult<T> {
        let response = self.fetch_with_retry(url).await?;
        let data: T = response.json().await?;
        Ok(data)
    }

    async fn get_json_paginated<T: DeserializeOwned + Send>(
        &self,
        url: &Url,
    ) -> HfResult<(T, bool)> {
        let response = self.fetch_with_retry(url).await?;

        // Check for pagination via Link header
        let has_more = response
            .headers()
            .get("Link")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|link| link.contains("rel=\"next\""));

        let data: T = response.json().await?;
        Ok((data, has_more))
    }
}

// ============================================================================
// Fake Backend for Testing
// ============================================================================

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Canned response for the fake backend.
    #[derive(Clone)]
    pub struct CannedResponse {
        pub json: serde_json::Value,
        pub has_more: bool,
    }

    /// A fake HTTP backend that returns canned responses.
    pub struct FakeBackend {
        responses: Arc<Mutex<HashMap<String, CannedResponse>>>,
        default_response: Option<CannedResponse>,
    }

    impl FakeBackend {
        /// Create a new fake backend.
        pub fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
                default_response: None,
            }
        }

        /// Add a canned response for a URL pattern.
        pub fn with_response(self, url_contains: &str, response: CannedResponse) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(url_contains.to_string(), response);
            self
        }

        /// Set a default response for URLs that don't match any pattern.
        pub fn with_default(mut self, response: CannedResponse) -> Self {
            self.default_response = Some(response);
            self
        }

        fn find_response(&self, url: &str) -> Option<CannedResponse> {
            {
                let responses = self.responses.lock().unwrap();
                for (pattern, response) in responses.iter() {
                    if url.contains(pattern) {
                        return Some(response.clone());
                    }
                }
            }
            self.default_response.clone()
        }
    }

    impl Default for FakeBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl HttpBackend for FakeBackend {
        async fn get_json<T: DeserializeOwned + Send>(&self, url: &Url) -> HfResult<T> {
            let response =
                self.find_response(url.as_str())
                    .ok_or_else(|| HfError::ApiRequestFailed {
                        status: 404,
                        url: url.to_string(),
                    })?;

            serde_json::from_value(response.json).map_err(Into::into)
        }

        async fn get_json_paginated<T: DeserializeOwned + Send>(
            &self,
            url: &Url,
        ) -> HfResult<(T, bool)> {
            let response =
                self.find_response(url.as_str())
                    .ok_or_else(|| HfError::ApiRequestFailed {
                        status: 404,
                        url: url.to_string(),
                    })?;

            let data: T = serde_json::from_value(response.json)?;
            Ok((data, response.has_more))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_id_from_path() {
        assert_eq!(
            extract_model_id_from_path("/api/models/TheBloke/Llama-2-7B-GGUF"),
            Some("TheBloke/Llama-2-7B-GGUF".to_string())
        );

        assert_eq!(
            extract_model_id_from_path("/api/models/TheBloke/Llama-2-7B-GGUF/tree/main"),
            Some("TheBloke/Llama-2-7B-GGUF".to_string())
        );

        assert_eq!(
            extract_model_id_from_path("api/models/Org/Model"),
            Some("Org/Model".to_string())
        );

        assert_eq!(extract_model_id_from_path("/api/models/"), None);
        assert_eq!(extract_model_id_from_path("/other/path"), None);
    }

    #[test]
    fn test_reqwest_backend_creation() {
        let config = HfConfig::default();
        let backend = ReqwestBackend::new(&config);
        assert_eq!(backend.max_retries, 3);
        assert_eq!(backend.retry_base_delay_ms, 500);
        assert!(backend.auth_token.is_none());
    }

    #[test]
    fn test_reqwest_backend_with_token() {
        let config = HfConfig {
            token: Some("test_token".to_string()),
            ..Default::default()
        };
        let backend = ReqwestBackend::new(&config);
        assert_eq!(backend.auth_token, Some("test_token".to_string()));
    }

    #[cfg(test)]
    mod fake_backend_tests {
        use super::testing::*;
        use super::*;
        use serde_json::json;

        #[tokio::test]
        async fn test_fake_backend_returns_canned_response() {
            let backend = FakeBackend::new().with_response(
                "test-model",
                CannedResponse {
                    json: json!({"id": "test-model", "downloads": 100}),
                    has_more: false,
                },
            );

            let url = Url::parse("https://example.com/api/test-model").unwrap();
            let result: serde_json::Value = backend.get_json(&url).await.unwrap();

            assert_eq!(result["id"], "test-model");
            assert_eq!(result["downloads"], 100);
        }

        #[tokio::test]
        async fn test_fake_backend_returns_404_for_unknown_url() {
            let backend = FakeBackend::new();
            let url = Url::parse("https://example.com/unknown").unwrap();

            let result: HfResult<serde_json::Value> = backend.get_json(&url).await;
            assert!(matches!(
                result,
                Err(HfError::ApiRequestFailed { status: 404, .. })
            ));
        }

        #[tokio::test]
        async fn test_fake_backend_default_response() {
            let backend = FakeBackend::new().with_default(CannedResponse {
                json: json!({"default": true}),
                has_more: false,
            });

            let url = Url::parse("https://example.com/anything").unwrap();
            let result: serde_json::Value = backend.get_json(&url).await.unwrap();

            assert_eq!(result["default"], true);
        }

        #[tokio::test]
        async fn test_fake_backend_paginated() {
            let backend = FakeBackend::new().with_response(
                "search",
                CannedResponse {
                    json: json!([{"id": "model1"}, {"id": "model2"}]),
                    has_more: true,
                },
            );

            let url = Url::parse("https://example.com/search?q=test").unwrap();
            let (result, has_more): (Vec<serde_json::Value>, bool) =
                backend.get_json_paginated(&url).await.unwrap();

            assert_eq!(result.len(), 2);
            assert!(has_more);
        }
    }
}
