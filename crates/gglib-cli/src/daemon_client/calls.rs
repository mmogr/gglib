//! The typed calls on a [`DaemonHandle`].
//!
//! Split from `mod.rs`, which owns the *connection* — finding the daemon,
//! launching it, checking its identity — when the remote tunnel's calls
//! arrived and that file was at its budget. Every method here is one route
//! constant from `gglib_core::contracts::http::daemon` and one body from
//! `wire`.

use std::time::Duration;

use anyhow::{Result, anyhow};
use gglib_app_services::types::QueueDownloadResponse;
use gglib_core::download::{DownloadId, QueueSnapshot};

use super::wire::{
    ProxyStatusDto, QueueDownloadBody, StartProxyBody, StartServerBody, StartServerDto,
};
use super::{DaemonHandle, auth, base_url, paths};

impl DaemonHandle {
    /// One absolute URL on the daemon.
    #[allow(
        clippy::unused_self,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    pub(super) fn url(&self, path: &str) -> String {
        format!("{}{path}", base_url())
    }

    /// A request to the daemon carrying this handle's credential.
    ///
    /// Every call goes through here rather than reaching for `self.client`, so
    /// a route added later cannot quietly skip the header. It is worth the
    /// indirection: the bearer layer is installed with `.layer`, so it answers
    /// before method routing, and a missing credential therefore looks exactly
    /// like a route that does not exist.
    pub(super) fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(method, self.url(path));
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        }
    }

    /// [`Self::request`] for a GET.
    pub(super) fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::GET, path)
    }

    /// [`Self::request`] for a POST.
    pub(super) fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::POST, path)
    }

    /// [`Self::request`] for a DELETE.
    pub(super) fn delete(&self, path: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::DELETE, path)
    }

    /// Read an HTTP response, surfacing non-2xx bodies as errors.
    pub(super) async fn expect_ok(response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!(
                "daemon answered 401: {}",
                auth::unauthorized(&body)
            ));
        }
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
            .post(paths::PROXY_START_PATH)
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop the proxy (idempotent on the daemon side).
    pub(crate) async fn stop_proxy(&self) -> Result<ProxyStatusDto> {
        let response = self
            .post(paths::PROXY_STOP_PATH)
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Current proxy status.
    pub(crate) async fn proxy_status(&self) -> Result<ProxyStatusDto> {
        let response = self
            .get(paths::PROXY_STATUS_PATH)
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
        body: &StartServerBody,
    ) -> Result<StartServerDto> {
        let response = self
            .post(paths::SERVERS_START_PATH)
            .json(body)
            .timeout(Duration::from_mins(3))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Queue a model download on the daemon, and answer the ID the daemon
    /// gave it: the download this request is, whatever else the queue holds.
    ///
    /// Long timeout: the daemon resolves the repo and its shard list against
    /// `HuggingFace` before answering.
    pub(crate) async fn queue_download(&self, body: &QueueDownloadBody) -> Result<DownloadId> {
        let response = self
            .post(paths::DOWNLOADS_QUEUE_PATH)
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        let queued: QueueDownloadResponse = Self::expect_ok(response).await?.json().await?;
        Ok(queued_id(&queued))
    }

    /// The daemon's download queue snapshot — what the dashboard renders.
    ///
    /// `GET` and `POST` share one path by design; the snapshot has no other
    /// mount ([#834]).
    ///
    /// [#834]: https://github.com/mmogr/gglib/pull/834
    pub(crate) async fn download_queue(&self) -> Result<QueueSnapshot> {
        let response = self
            .get(paths::DOWNLOADS_QUEUE_PATH)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// The daemon's setup status as the JSON it answers with, whatever the
    /// status code: the benchmark report reads this machine's hardware from
    /// it, and has nothing to say about a refusal.
    pub(crate) async fn setup_status(&self) -> Result<serde_json::Value> {
        let response = self.get(paths::SETUP_STATUS_PATH).send().await?;
        Ok(response.json().await?)
    }

    /// The request that asks the daemon to judge tune run `run_id` against the
    /// apply gate, carrying this handle's credential. A builder, because the
    /// caller reads a refusal as a verdict rather than an error.
    pub(crate) fn tune_apply(&self, run_id: i64) -> reqwest::RequestBuilder {
        self.post(&paths::benchmark_tune_apply_path(run_id))
    }

    /// Ask the daemon to shut down. `Ok(true)` when a shutdown was accepted,
    /// `Ok(false)` when the server said it is not running as a daemon.
    ///
    /// The 401 arm is not redundant with the `Ok(false)` one. This is the only
    /// call that reads the status itself rather than going through
    /// [`Self::expect_ok`], so without it a refused credential would arrive as
    /// `Ok(false)` and print "not running as a daemon" — which is a different
    /// problem with a different remedy.
    pub(crate) async fn shutdown_daemon(&self) -> Result<bool> {
        let response = self
            .post(paths::DAEMON_SHUTDOWN_PATH)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "daemon answered 401: {}",
                auth::unauthorized(&body)
            ));
        }
        Ok(response.status() == reqwest::StatusCode::ACCEPTED)
    }
}

/// The download a queue request was answered with.
fn queued_id(queued: &QueueDownloadResponse) -> DownloadId {
    DownloadId::from(queued.id.as_str())
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::TcpListener;

    use super::*;
    use crate::daemon_client::STAND_IN_PORT;
    use crate::handlers::agent_chat::sight::sight_tests::read_request;

    /// A queue request is a POST of the body to the queue route, and its
    /// answer is the ID in the daemon's reply: here one with a quantization
    /// the request did not name.
    #[tokio::test]
    async fn a_queue_request_answers_the_id_the_daemon_replied_with() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("its address").port();
        let daemon = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the request");
            let request = read_request(&mut stream);
            let reply = r#"{"id":"owner/repo:Q8_0"}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
            request
        });
        let handle = DaemonHandle {
            client: gglib_proxy::loopback::client(),
            api_key: None,
        };
        let body = QueueDownloadBody {
            model_id: "owner/repo".to_owned(),
            quant: None,
        };

        let queued = STAND_IN_PORT
            .scope(port, handle.queue_download(&body))
            .await
            .expect("the daemon's answer");

        assert_eq!(queued, DownloadId::new("owner/repo", Some("Q8_0")));
        let (line, sent) = daemon.join().expect("the stand-in ran");
        let route = paths::DOWNLOADS_QUEUE_PATH;
        assert_eq!(line, format!("POST {route} HTTP/1.1"));
        let sent: serde_json::Value = serde_json::from_str(&sent).expect("a JSON body");
        assert_eq!(sent["model_id"], "owner/repo");
        assert!(sent["quant"].is_null(), "{sent}");
    }

    /// The ID read from the daemon's answer is the one the daemon wrote: a
    /// quantization stays part of it, and a repository alone is itself.
    #[test]
    fn the_queued_id_is_the_one_the_daemon_answered() {
        let answer = |id: &str| -> QueueDownloadResponse {
            serde_json::from_value(serde_json::json!({ "id": id })).expect("the daemon's shape")
        };

        let with_quant = queued_id(&answer("owner/repo:Q8_0"));
        assert_eq!(with_quant, DownloadId::new("owner/repo", Some("Q8_0")));
        assert_eq!(with_quant.to_string(), "owner/repo:Q8_0");
        assert_eq!(queued_id(&answer("owner/repo")).to_string(), "owner/repo");
    }

    /// Every call carries the credential, the one that posts a tune's verdict
    /// included: it was the one raw post left, and 401'd on every daemon.
    #[test]
    fn the_tune_apply_request_carries_the_credential() {
        let handle = DaemonHandle {
            client: gglib_proxy::loopback::client(),
            api_key: Some("the-token".to_owned()),
        };
        let request = handle.tune_apply(7).build().expect("a request");

        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(request.url().path(), paths::benchmark_tune_apply_path(7));
        let auth = request.headers().get(reqwest::header::AUTHORIZATION);
        assert_eq!(auth.and_then(|v| v.to_str().ok()), Some("Bearer the-token"));
    }
}
