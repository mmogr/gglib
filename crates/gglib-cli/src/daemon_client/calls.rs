//! The typed calls on a [`DaemonHandle`].
//!
//! Split from `mod.rs`, which owns the *connection* — finding the daemon,
//! launching it, checking its identity — when the remote tunnel's calls
//! arrived and that file was at its budget. Every method here is one route
//! constant from `gglib_core::contracts::http::daemon` and one body from
//! `wire`.

use std::time::Duration;

use anyhow::{Result, anyhow};
use gglib_core::download::QueueSnapshot;

use super::wire::{ProxyStatusDto, QueueDownloadBody, StartProxyBody, StartServerDto};
use super::{DaemonHandle, base_url, paths};

impl DaemonHandle {
    /// One absolute URL on the daemon.
    pub(super) fn url(&self, path: &str) -> String {
        format!("{}{path}", base_url())
    }

    /// Read an HTTP response, surfacing non-2xx bodies as errors.
    pub(super) async fn expect_ok(response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        // The daemon's error envelope is {"error": "..."} — surface just the
        // message when it parses, the raw body otherwise.
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or(body);
        Err(anyhow!("daemon answered {status}: {message}"))
    }

    /// Start the proxy (idempotent on the daemon side).
    pub(crate) async fn start_proxy(&self, body: &StartProxyBody) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url(paths::PROXY_START_PATH))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop the proxy (idempotent on the daemon side).
    pub(crate) async fn stop_proxy(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url(paths::PROXY_STOP_PATH))
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Current proxy status.
    pub(crate) async fn proxy_status(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .get(self.url(paths::PROXY_STATUS_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Start (or reuse) a llama-server for a model, returning its port.
    ///
    /// Long timeout: the daemon holds the request open while the model loads.
    pub(crate) async fn start_model_server(
        &self,
        model_id: i64,
        context_length: Option<u64>,
    ) -> Result<StartServerDto> {
        let response = self
            .client
            .post(self.url(paths::SERVERS_START_PATH))
            .json(&serde_json::json!({ "id": model_id, "context_length": context_length }))
            .timeout(Duration::from_secs(180))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Queue a model download on the daemon.
    ///
    /// Long timeout: the daemon resolves the repo and its shard list against
    /// HuggingFace before answering.
    /// The response body carries a queue position and shard count. Nothing
    /// reads them — the caller goes straight to watching the queue — so this
    /// checks the status and discards the body rather than deserializing a
    /// shape no one inspects.
    pub(crate) async fn queue_download(&self, body: &QueueDownloadBody) -> Result<()> {
        let response = self
            .client
            .post(self.url(paths::DOWNLOADS_QUEUE_PATH))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Self::expect_ok(response).await?;
        Ok(())
    }

    /// The daemon's download queue snapshot — what the dashboard renders.
    ///
    /// `GET` and `POST` share one path by design. The snapshot handler was once
    /// double-mounted at `/api/models/downloads` as well, and when that second
    /// mount was retired as unused (#834) the CLI was still polling it. The bare
    /// path then fell through to `/api/models/{id}`, whose `i64` extractor
    /// answers `400 text/plain` — which the poller tried to parse as JSON.
    pub(crate) async fn download_queue(&self) -> Result<QueueSnapshot> {
        let response = self
            .client
            .get(self.url(paths::DOWNLOADS_QUEUE_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Ask the daemon to shut down. `Ok(true)` when a shutdown was accepted,
    /// `Ok(false)` when the server said it is not running as a daemon.
    pub(crate) async fn shutdown_daemon(&self) -> Result<bool> {
        let response = self
            .client
            .post(self.url(paths::DAEMON_SHUTDOWN_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(response.status() == reqwest::StatusCode::ACCEPTED)
    }
}
