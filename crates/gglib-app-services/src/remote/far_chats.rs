//! The far machine's chats and runs, read and carried on *through* the
//! tunnel for this machine's chat page.
//!
//! Each request goes to the far proxy over the local listener with the key
//! `join` stored, as [`super::far_daemon`]'s shutdown does: the listener
//! adds no credential (ADR 0012, decision 7), so this side attaches it. The
//! answers are handed back whole for the daemon to pass on; nothing here
//! keeps a row, a title or a message, and nothing here logs one.
//!
//! A run's events are a stream that lasts as long as its reply, so they go
//! through a client with no overall timeout, only a limit on silence; every
//! other request is bounded. The two clients are built once and shared: the
//! page asks for the far runs every few seconds.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::runs::is_run_id;

use crate::error::GuiError;

/// How long a bounded request may take end to end. Generous for the same
/// reason as the shutdown's: a first request may still be finishing the hole
/// punch.
const TIMEOUT: Duration = Duration::from_secs(20);

/// How long a stream may take to connect. Once it has, it lasts as long as
/// the run does.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a stream may go without a byte before it is taken as dead. The
/// far proxy sends a keep-alive comment every 15 seconds (axum's default),
/// so this is three of them missed.
pub(super) const STREAM_READ_TIMEOUT: Duration = Duration::from_secs(45);

/// A stream's limit end to end: none. A reply takes as long as it takes.
pub(super) const STREAM_TOTAL_TIMEOUT: Option<Duration> = None;

/// What a request that did not get through says: fixed text, never the
/// client's own wording.
pub(super) const NO_ANSWER: &str = "the other machine did not answer";

/// The far machine's proxy, as the chat page reads it: its base URL through
/// the tunnel and the key it admits this device by.
#[derive(Clone)]
pub struct FarChats {
    base_url: String,
    key: String,
    bounded: reqwest::Client,
    streaming: reqwest::Client,
}

/// Never the key.
impl fmt::Debug for FarChats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FarChats")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl FarChats {
    /// The far proxy at `base_url` (`http://127.0.0.1:<port>/v1`), reached
    /// with `key`.
    ///
    /// # Errors
    ///
    /// `Internal` when an HTTP client cannot be built.
    pub fn new(base_url: &str, key: &str) -> Result<Self, GuiError> {
        static CLIENTS: OnceLock<(reqwest::Client, reqwest::Client)> = OnceLock::new();
        let (bounded, streaming) = match CLIENTS.get() {
            Some(clients) => clients.clone(),
            None => {
                let built = (
                    build(gglib_proxy::loopback::client_builder().timeout(TIMEOUT))?,
                    build(streaming_builder(STREAM_READ_TIMEOUT))?,
                );
                CLIENTS.get_or_init(|| built).clone()
            }
        };
        Ok(Self::with_clients(base_url, key, bounded, streaming))
    }

    /// The far proxy at `base_url`, through the clients given.
    pub(super) fn with_clients(
        base_url: &str,
        key: &str,
        bounded: reqwest::Client,
        streaming: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
            bounded,
            streaming,
        }
    }

    /// `GET /v1/chats`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn list_chats(&self) -> Result<reqwest::Response, GuiError> {
        self.send(self.bounded.get(self.url("/chats"))).await
    }

    /// `GET /v1/chats/{id}`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn open_chat(&self, id: i64) -> Result<reqwest::Response, GuiError> {
        self.send(self.bounded.get(self.url(&format!("/chats/{id}"))))
            .await
    }

    /// `PUT /v1/runs/{run_id}?kind=agent` with `{conversation_id, content}`:
    /// a turn on one of the far machine's chats, which it runs and saves.
    ///
    /// # Errors
    ///
    /// `ValidationFailed` for a run id the hub would not accept, before
    /// anything is sent; `Unavailable` when the request did not get through.
    pub async fn add_turn(
        &self,
        run_id: &str,
        turn: &HubTurn,
    ) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}?kind=agent", run_path(run_id)?);
        self.send(self.bounded.put(self.url(&path)).json(turn))
            .await
    }

    /// `GET /v1/runs`: the runs this device may see there.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn list_runs(&self) -> Result<reqwest::Response, GuiError> {
        self.send(self.bounded.get(self.url("/runs"))).await
    }

    /// `POST /v1/runs/{run_id}/cancel`.
    ///
    /// # Errors
    ///
    /// As [`FarChats::add_turn`].
    pub async fn cancel_run(&self, run_id: &str) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}/cancel", run_path(run_id)?);
        self.send(self.bounded.post(self.url(&path))).await
    }

    /// `GET /v1/runs/{run_id}/events?after={after}`, answered as soon as its
    /// head arrives: the body is the stream, read as the run goes.
    ///
    /// # Errors
    ///
    /// As [`FarChats::add_turn`].
    pub async fn run_events(
        &self,
        run_id: &str,
        after: u32,
    ) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}/events?after={after}", run_path(run_id)?);
        let request = self
            .streaming
            .get(self.url(&path))
            .header(reqwest::header::ACCEPT, "text/event-stream");
        self.send(request).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, GuiError> {
        request.bearer_auth(&self.key).send().await.map_err(|e| {
            tracing::debug!(error = %e, "a request to the far machine did not get through");
            GuiError::Unavailable(NO_ANSWER.to_owned())
        })
    }
}

/// Build a client, or say it could not be.
pub(super) fn build(builder: reqwest::ClientBuilder) -> Result<reqwest::Client, GuiError> {
    builder
        .build()
        .map_err(|e| GuiError::Internal(format!("could not build an HTTP client: {e}")))
}

/// The client a stream is read through: bounded on connecting and on
/// silence, never end to end.
pub(super) fn streaming_builder(read_timeout: Duration) -> reqwest::ClientBuilder {
    let builder = gglib_proxy::loopback::client_builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(read_timeout);
    match STREAM_TOTAL_TIMEOUT {
        Some(total) => builder.timeout(total),
        None => builder,
    }
}

/// `/runs/{run_id}`, for an id the hub accepts: never a path a caller built.
fn run_path(run_id: &str) -> Result<String, GuiError> {
    if !is_run_id(run_id) {
        return Err(GuiError::ValidationFailed(
            "a run id is 1 to 64 of A-Z, a-z, 0-9, '_' and '-'".to_owned(),
        ));
    }
    Ok(format!("/runs/{run_id}"))
}

#[cfg(test)]
#[path = "far_chats_tests.rs"]
mod far_chats_tests;
