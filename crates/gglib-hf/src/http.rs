//! HTTP backend abstraction for `HuggingFace` API.
//!
//! This module provides a trait-based HTTP backend that allows for
//! dependency injection and easy testing. The production implementation
//! uses reqwest with automatic retry logic for transient errors.

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

    /// Fetch at most the first `max_bytes` bytes of a URL's body, asking
    /// with a `Range` header and stopping there even when the server answers
    /// with the whole body.
    async fn get_head(&self, url: &Url, max_bytes: u64) -> HfResult<Vec<u8>>;

    /// Post `form` to a URL as `application/x-www-form-urlencoded` and
    /// deserialize the JSON answer.
    async fn post_form_json<T: DeserializeOwned + Send>(
        &self,
        url: &Url,
        form: &[(&str, &str)],
    ) -> HfResult<T>;
}

/// `form` encoded as a URL-encoded form body, in its order.
pub(crate) fn form_body(form: &[(&str, &str)]) -> String {
    form.iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
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

    /// `request` with the client's token on it, when it has one.
    fn authorized(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.auth_token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        request
    }

    /// Build a GET request with optional authentication.
    fn build_request(&self, url: &Url) -> reqwest::RequestBuilder {
        self.authorized(self.client.get(url.as_str()))
    }

    /// Fetch a URL with automatic retry for transient errors.
    async fn fetch_with_retry(&self, url: &Url) -> HfResult<reqwest::Response> {
        self.send_with_retry(url, || self.build_request(url)).await
    }

    /// Send the request `build` makes, built afresh for each attempt, with
    /// automatic retry for transient errors.
    async fn send_with_retry(
        &self,
        url: &Url,
        build: impl Fn() -> reqwest::RequestBuilder + Send + Sync,
    ) -> HfResult<reqwest::Response> {
        let mut last_error: Option<HfError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let delay = Duration::from_millis(
                    self.retry_base_delay_ms * 2u64.pow(u32::from(attempt) - 1),
                );
                tokio::time::sleep(delay).await;
            }

            match build().send().await {
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

    async fn get_head(&self, url: &Url, max_bytes: u64) -> HfResult<Vec<u8>> {
        if max_bytes == 0 {
            return Ok(Vec::new());
        }
        let range = format!("bytes=0-{}", max_bytes - 1);
        let mut response = self
            .send_with_retry(url, || {
                self.build_request(url).header("Range", range.as_str())
            })
            .await?;
        let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        let mut head = Vec::new();
        while head.len() < cap {
            let Some(chunk) = response.chunk().await? else {
                break;
            };
            let room = cap - head.len();
            head.extend_from_slice(&chunk[..chunk.len().min(room)]);
        }
        Ok(head)
    }

    async fn post_form_json<T: DeserializeOwned + Send>(
        &self,
        url: &Url,
        form: &[(&str, &str)],
    ) -> HfResult<T> {
        let body = form_body(form);
        let response = self
            .send_with_retry(url, || {
                self.authorized(self.client.post(url.as_str()))
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body(body.clone())
            })
            .await?;
        let data: T = response.json().await?;
        Ok(data)
    }
}

// ============================================================================
// Fake Backend for Testing
// ============================================================================

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Canned response for the fake backend.
    #[derive(Clone)]
    pub(crate) struct CannedResponse {
        pub json: serde_json::Value,
        pub has_more: bool,
    }

    /// One request the fake backend was asked, as it was asked.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Asked {
        /// A ranged read of a URL, for at most this many bytes.
        Head(String, u64),
        /// A form posted to a URL, as its encoded body.
        Form(String, String),
    }

    /// A fake HTTP backend that returns canned responses.
    pub(crate) struct FakeBackend {
        responses: Arc<Mutex<HashMap<String, CannedResponse>>>,
        default_response: Option<CannedResponse>,
        bodies: HashMap<String, Vec<u8>>,
        asked: Mutex<Vec<Asked>>,
    }

    impl FakeBackend {
        /// Create a new fake backend.
        pub(crate) fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
                default_response: None,
                bodies: HashMap::new(),
                asked: Mutex::new(Vec::new()),
            }
        }

        /// Serve `body` to a ranged read of a URL containing `url_contains`.
        pub(crate) fn with_body(mut self, url_contains: &str, body: &[u8]) -> Self {
            self.bodies.insert(url_contains.to_string(), body.to_vec());
            self
        }

        /// The ranged reads and posted forms asked of it, oldest first.
        pub(crate) fn asked(&self) -> Vec<Asked> {
            self.asked.lock().unwrap().clone()
        }

        /// Add a canned response for a URL pattern.
        pub(crate) fn with_response(self, url_contains: &str, response: CannedResponse) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(url_contains.to_string(), response);
            self
        }

        /// Set a default response for URLs that don't match any pattern.
        pub(crate) fn with_default(mut self, response: CannedResponse) -> Self {
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

        async fn get_head(&self, url: &Url, max_bytes: u64) -> HfResult<Vec<u8>> {
            self.asked
                .lock()
                .unwrap()
                .push(Asked::Head(url.to_string(), max_bytes));
            let body = self
                .bodies
                .iter()
                .find_map(|(pattern, body)| url.as_str().contains(pattern.as_str()).then_some(body))
                .ok_or_else(|| HfError::ApiRequestFailed {
                    status: 404,
                    url: url.to_string(),
                })?;
            let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
            Ok(body[..body.len().min(cap)].to_vec())
        }

        async fn post_form_json<T: DeserializeOwned + Send>(
            &self,
            url: &Url,
            form: &[(&str, &str)],
        ) -> HfResult<T> {
            self.asked
                .lock()
                .unwrap()
                .push(Asked::Form(url.to_string(), form_body(form)));
            self.get_json(url).await
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
