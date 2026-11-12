//! The far proxy's answers this side reads rather than passes on: its model
//! list, one model, a load, and the shutdown.
//!
//! A `#[path]` child of `far_proxy.rs`. The models are read into the types
//! the far proxy wrote them from (`gglib_proxy::models::ModelsResponse`,
//! `ModelLookup`, `gglib_proxy::LoadResponse`), so a reader on this side and
//! the writer on that side cannot disagree about a field. A far refusal is
//! kept whole as [`FarError::Refused`], for the daemon to hand on in its own
//! shape.
//!
//! A far machine whose list has no `gglib` object runs a gglib from before
//! devices said their version, and is refused with one sentence asking for it
//! to be updated (`far_said.rs`), before its models are read: an older list
//! may not read as this build's at all. Nothing is shimmed.

use std::time::Duration;

use gglib_core::contracts::http::path_segment;
use gglib_core::domain::ModelLookup;
use gglib_proxy::LoadResponse;
use gglib_proxy::models::ModelsResponse;
use reqwest::StatusCode;
use reqwest::header::{HeaderValue, RETRY_AFTER};
use serde::de::DeserializeOwned;
use tracing::info;

use super::FarProxy;
use crate::error::GuiError;

/// How long the model list may take end to end. Short, because a surface
/// reads it to draw a page and must not wait out a tunnel that is not there.
const LIST_TIMEOUT: Duration = Duration::from_secs(3);

/// How long a load may take end to end: the three minutes the far proxy's
/// admission waits before it answers either way, and one more for the
/// tunnel and the load itself.
const LOAD_TIMEOUT: Duration = Duration::from_mins(4);

/// Why a read of the far proxy came back with no answer.
#[derive(Debug)]
pub enum FarError {
    /// The far proxy refused: its status, its `Retry-After` and its body as
    /// they came, for the daemon to pass on in its own shape.
    Refused {
        /// The far proxy's status.
        status: StatusCode,
        /// When it said to come back, if it did.
        retry_after: Option<HeaderValue>,
        /// Its body: `{"error": {"message", "code"}}` from a gglib proxy.
        body: Vec<u8>,
    },
    /// Anything else: the request did not get through, the answer did not
    /// read, or the far machine does not serve what was asked.
    Failed(GuiError),
}

impl From<GuiError> for FarError {
    fn from(error: GuiError) -> Self {
        Self::Failed(error)
    }
}

impl FarProxy {
    /// `GET /v1/models`: every entry the far proxy publishes, profile
    /// variants included, and the far machine's name when it gave one.
    ///
    /// # Errors
    ///
    /// [`FarError::Refused`] for a far refusal; `Conflict`, asking for an
    /// update, for a list with no `gglib` object; `Unavailable` when the
    /// request did not get through in three seconds, or the list does not
    /// read.
    pub async fn models(&self) -> Result<ModelsResponse, FarError> {
        let (body, about) = self.listed().await?;
        if about.gglib.is_none() {
            return Err(FarError::Failed(self.older()));
        }
        decode(&body)
    }

    /// The model list's body, as it came.
    pub(super) async fn list_body(&self) -> Result<Vec<u8>, FarError> {
        let request = self.bounded.get(self.url("/models")).timeout(LIST_TIMEOUT);
        accepted(self.send(request).await?).await
    }

    /// `GET /v1/models/{identifier}/detail`: one model, resolved there as a
    /// turn sent with `identifier` would be. The identifier travels as one
    /// path segment, so a name holding `/` or `:` arrives whole.
    ///
    /// # Errors
    ///
    /// [`FarError::Refused`] for a far refusal; `Conflict`, asking for an
    /// update, for a gglib from before devices said their version;
    /// `Unavailable` as for [`FarProxy::models`].
    pub async fn lookup(&self, identifier: &str) -> Result<ModelLookup, FarError> {
        self.offers(&[]).await?;
        let path = format!("/models/{}/detail", path_segment(identifier));
        decode(&accepted(self.send(self.bounded.get(self.url(&path))).await?).await?)
    }

    /// `POST /v1/models/{identifier}/load` with `{num_ctx}`: have the model
    /// resident now, so the first turn does not wait.
    ///
    /// # Errors
    ///
    /// [`FarError::Refused`] for a far refusal; `Conflict` as for
    /// [`FarProxy::lookup`]; `Unavailable` as for [`FarProxy::models`].
    pub async fn load(
        &self,
        identifier: &str,
        num_ctx: Option<u64>,
    ) -> Result<LoadResponse, FarError> {
        self.offers(&[]).await?;
        let path = format!("/models/{}/load", path_segment(identifier));
        let request = self
            .bounded
            .post(self.url(&path))
            .timeout(LOAD_TIMEOUT)
            .json(&serde_json::json!({ "num_ctx": num_ctx }));
        decode(&accepted(self.send(request).await?).await?)
    }

    /// Stop the far daemon: `POST /v1/proxy/shutdown` with the confirmation
    /// word the route requires (ADR 0012, decision 7). A one-way door, so it
    /// is not retried.
    ///
    /// # Errors
    ///
    /// `ValidationFailed` when the key is refused, `Conflict` when the far
    /// proxy is not running under a daemon or as for [`FarProxy::lookup`],
    /// `Unavailable` when the request did not get through.
    pub async fn shutdown(&self) -> Result<(), GuiError> {
        self.offers(&[]).await?;
        let request = self
            .bounded
            .post(self.url("/proxy/shutdown"))
            .json(&serde_json::json!({ "confirm": "shutdown" }));
        match self.send(request).await?.status() {
            s if s.is_success() => {
                info!("the remote daemon accepted the shutdown");
                Ok(())
            }
            // The claim the chat path's refusal makes too.
            StatusCode::UNAUTHORIZED => Err(GuiError::ValidationFailed(self.key_refused())),
            StatusCode::CONFLICT => Err(GuiError::Conflict(
                "the far proxy is not running under a daemon, so there is nothing to stop from \
                 here"
                    .to_owned(),
            )),
            s => Err(GuiError::Unavailable(format!(
                "the shutdown request was answered with {s}"
            ))),
        }
    }
}

/// A success's body, or the refusal kept whole.
async fn accepted(answer: reqwest::Response) -> Result<Vec<u8>, FarError> {
    let status = answer.status();
    let retry_after = answer.headers().get(RETRY_AFTER).cloned();
    let body = answer
        .bytes()
        .await
        .map_err(|_| GuiError::Unavailable("the other machine's answer was cut off".to_owned()))?
        .to_vec();
    if status.is_success() {
        Ok(body)
    } else {
        Err(FarError::Refused {
            status,
            retry_after,
            body,
        })
    }
}

fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, FarError> {
    serde_json::from_slice(body).map_err(|e| unreadable(&e))
}

/// An answer this build cannot read. Not the update sentence: a far machine
/// on a newer build than this one may have changed a shape this one knows.
pub(super) fn unreadable(e: &serde_json::Error) -> FarError {
    FarError::Failed(GuiError::Unavailable(format!(
        "the other machine answered something this build cannot read: {e}"
    )))
}
