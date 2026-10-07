//! A chat run: one streamed chat completion through the daemon's own proxy.
//!
//! The body goes to `POST /v1/chat/completions` with `stream` and
//! `return_progress` forced on, and every `data:` payload is logged verbatim
//! as one event, except the closing `[DONE]`. The proxy not running fails
//! the run; this never starts it.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use gglib_core::domain::runs::RunError;
use gglib_core::sse::DataFrames;
use gglib_proxy::models::ErrorResponse;
use serde_json::Value;

use super::cell::LOG_LIMIT;
use super::door::{ProxyDoor, dial};
use super::executor::{RunExecutor, RunLog};

/// How long connecting to the proxy may take. The reply itself is not
/// bounded: a run lasts as long as its reply.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The error a run ends with, from fixed text.
fn run_error(code: &str, message: &str) -> RunError {
    RunError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// Runs a chat completion through the daemon's proxy.
pub(crate) struct ChatExecutor {
    door: Arc<dyn ProxyDoor>,
    client: reqwest::Client,
    /// The most one incomplete event may hold before the run fails: the
    /// log's own limit, since such an event could never be logged.
    event_limit: usize,
}

impl ChatExecutor {
    pub(crate) fn new(door: Arc<dyn ProxyDoor>) -> Self {
        let client = gglib_proxy::loopback::client_builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| gglib_proxy::loopback::client());
        Self {
            door,
            client,
            event_limit: LOG_LIMIT,
        }
    }

    /// The same, with a smaller event limit, so a test need not send 8 MB.
    #[cfg(test)]
    pub(crate) fn with_event_limit(door: Arc<dyn ProxyDoor>, event_limit: usize) -> Self {
        Self {
            event_limit,
            ..Self::new(door)
        }
    }
}

#[async_trait]
impl RunExecutor for ChatExecutor {
    async fn execute(&self, mut body: Value, log: RunLog) -> Result<(), RunError> {
        let Some(door) = self.door.open().await? else {
            return Err(run_error(
                "proxy_not_running",
                "The proxy is not running. Start it with `gglib proxy`.",
            ));
        };
        if let Some(fields) = body.as_object_mut() {
            fields.insert("stream".to_owned(), Value::Bool(true));
            fields.insert("return_progress".to_owned(), Value::Bool(true));
        }
        let url = format!("http://{}/v1/chat/completions", dial(door.addr));
        let mut request = self.client.post(url).json(&body);
        if let Some(key) = &door.key {
            request = request.bearer_auth(key);
        }
        let response = request
            .send()
            .await
            .map_err(|_| run_error("upstream_error", "The proxy could not be reached."))?;
        if !response.status().is_success() {
            return Err(refusal(response).await);
        }
        log.started();
        read_reply(response, &log, self.event_limit).await
    }
}

/// The run error for a non-2xx answer: the proxy's own code and message.
async fn refusal(response: reqwest::Response) -> RunError {
    let status = response.status();
    let body = response.bytes().await.unwrap_or_default();
    match serde_json::from_slice::<ErrorResponse>(&body) {
        Ok(answer) => {
            let detail = answer.error;
            let code = detail
                .code
                .filter(|c| !c.is_empty())
                .or_else(|| Some(detail.r#type).filter(|t| !t.is_empty()))
                .unwrap_or_else(|| "upstream_error".to_owned());
            RunError {
                code,
                message: detail.message,
            }
        }
        Err(_) => RunError {
            code: "upstream_error".to_owned(),
            message: format!("The proxy answered {status}."),
        },
    }
}

/// Log the reply's events until `[DONE]`.
async fn read_reply(
    response: reqwest::Response,
    log: &RunLog,
    event_limit: usize,
) -> Result<(), RunError> {
    let mut stream = response.bytes_stream();
    let mut frames = DataFrames::new(event_limit);
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| run_error("upstream_error", "The reply stream broke off."))?;
        for data in frames.push(&chunk) {
            if data == "[DONE]" {
                return Ok(());
            }
            let failure = in_stream_error(&data);
            if log.append(data).is_err() {
                // The run already ended (its log filled); the registry has
                // its final state, and this is dropped with the stream.
                return Ok(());
            }
            if let Some(error) = failure {
                return Err(error);
            }
        }
        if frames.overflowed() {
            return Err(run_error(
                "log_full",
                "One event of the reply passed the 8 MB a run may hold.",
            ));
        }
    }
    Err(run_error(
        "upstream_error",
        "The reply ended before it was finished.",
    ))
}

/// The run error an error frame carries, when `data` is one.
fn in_stream_error(data: &str) -> Option<RunError> {
    let value: Value = serde_json::from_str(data).ok()?;
    let error = value.get("error")?;
    let text = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let message = error
        .as_str()
        .map(str::to_owned)
        .or_else(|| text("message"))
        .unwrap_or_else(|| "The proxy reported an error.".to_owned());
    let code = text("code")
        .or_else(|| text("type"))
        .unwrap_or_else(|| "upstream_error".to_owned());
    Some(RunError { code, message })
}
