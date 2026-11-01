//! The far machine's proxy, reached *through* the tunnel: where it is, the
//! key it admits this machine by, which machine it is, and the name it goes
//! by. The routes are each in the file of the feature that asks them:
//! `far_chats.rs`, `far_runs.rs`, `far_images.rs`, `far_attachments.rs` and
//! `far_read.rs`, each a `#[path]` child that adds to `impl FarProxy`.
//!
//! Each request goes to the far proxy over the local listener with the key
//! `join` stored, which [`far_credentials`](super::paired_machine::far_credentials)
//! hands out only for the machine connected to: the listener adds no
//! credential (ADR 0012, decision 7), so this side attaches it. Nothing here
//! keeps a row, a title or a message, and nothing here logs one.
//!
//! A run's events are a stream that lasts as long as its reply, so they go
//! through a client with no overall timeout, only a limit on silence; every
//! other request is bounded. The two clients are built once and shared: the
//! page asks for the far runs every few seconds.

use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use gglib_core::domain::{Machine, UNNAMED_PAIRED};
use gglib_runtime::FarMachine;

use super::paired_machine::FarCredentials;
use crate::error::GuiError;

#[path = "far_attachments.rs"]
mod far_attachments;
#[path = "far_chats.rs"]
mod far_chats;
#[path = "far_images.rs"]
mod far_images;
#[path = "far_read.rs"]
mod far_read;
#[path = "far_runs.rs"]
mod far_runs;
pub use far_read::FarError;

/// How long a bounded request may take end to end. Generous because a first
/// request may still be finishing the hole punch; bounded because a tunnel
/// that never answers is a failure to report, not to wait out.
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

/// The far machine's proxy: its base URL through the tunnel, the key it
/// admits this device by, which machine it is, and the name it goes by.
#[derive(Clone)]
pub struct FarProxy {
    base_url: String,
    key: String,
    /// The ticket fingerprint of the machine connected to. Always a paired
    /// machine, so it is kept as the fingerprint and handed out as a
    /// [`Machine`] by [`FarProxy::machine`]. Compared, never shown.
    fingerprint: String,
    /// The name the stored pairing has for it, which is what is shown.
    name: Option<String>,
    bounded: reqwest::Client,
    streaming: reqwest::Client,
}

/// Never the key.
impl fmt::Debug for FarProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FarProxy")
            .field("base_url", &self.base_url)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl FarProxy {
    /// The far proxy at `base_url` (`http://127.0.0.1:<port>/v1`), reached
    /// with `credentials`.
    ///
    /// # Errors
    ///
    /// `Internal` when an HTTP client cannot be built.
    pub fn new(base_url: &str, credentials: &FarCredentials) -> Result<Self, GuiError> {
        static CLIENTS: OnceLock<(reqwest::Client, reqwest::Client)> = OnceLock::new();
        let (bounded, streaming) = if let Some(clients) = CLIENTS.get() {
            clients.clone()
        } else {
            let built = (
                build(gglib_proxy::loopback::client_builder().timeout(TIMEOUT))?,
                build(streaming_builder(STREAM_READ_TIMEOUT))?,
            );
            CLIENTS.get_or_init(|| built).clone()
        };
        Ok(Self::with_clients(
            base_url,
            credentials,
            bounded,
            streaming,
        ))
    }

    /// The far proxy at `base_url`, through the clients given.
    pub(super) fn with_clients(
        base_url: &str,
        credentials: &FarCredentials,
        bounded: reqwest::Client,
        streaming: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            key: credentials.key.clone(),
            fingerprint: credentials.fingerprint.clone(),
            name: credentials.name.clone(),
            bounded,
            streaming,
        }
    }

    /// The machine this proxy is on.
    #[must_use]
    pub fn machine(&self) -> Machine {
        Machine::Paired {
            fingerprint: self.fingerprint.clone(),
        }
    }

    /// This proxy, when `machine` is the one it is on. A model picked from
    /// the paired machine's list is named by that machine, and is sent
    /// nowhere else: a pairing replaced since the pick is another machine,
    /// whose same id is another model.
    ///
    /// # Errors
    ///
    /// `Conflict` when `machine` is not this proxy's.
    pub fn serving(self, machine: &Machine) -> Result<Self, GuiError> {
        if self.machine() == *machine {
            return Ok(self);
        }
        Err(GuiError::Conflict(
            "the paired machine changed since this model was picked — pick it again from the \
             paired machine's models, or `gglib remote join` the machine it is on"
                .to_owned(),
        ))
    }

    /// What a sentence about this machine calls it: its name, or
    /// [`UNNAMED_PAIRED`] when it has given none. Never its fingerprint.
    #[must_use]
    pub fn shown_name(&self) -> &str {
        self.name.as_deref().unwrap_or(UNNAMED_PAIRED)
    }

    /// `http://127.0.0.1:<port>`, the far proxy without the `/v1`: where an
    /// agent turn's completion adapter points, since it adds the `/v1` itself.
    #[must_use]
    pub fn server_root(&self) -> String {
        self.base_url
            .strip_suffix("/v1")
            .unwrap_or(&self.base_url)
            .to_owned()
    }

    /// The key and the name the completion adapter carries, so a refusal of
    /// the key names the machine that refused it.
    #[must_use]
    pub fn far_machine(&self) -> FarMachine {
        FarMachine {
            key: self.key.clone(),
            name: self.shown_name().to_owned(),
        }
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

#[cfg(test)]
#[path = "far_proxy_tests.rs"]
mod far_proxy_tests;
