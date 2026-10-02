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
//! A far machine on a build from before models carried ids is told apart
//! here, once, and refused with one sentence: its list does not read, because
//! `gglib_id` is missing, and its model detail route does not exist, so it
//! answers a 404 with no error code. Nothing is shimmed: that machine is
//! asked to update.

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

/// What a far machine on a build that publishes no model ids is told.
const OLDER_FAR: &str =
    "the paired machine runs an older gglib that publishes no model ids — update it";

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
    /// read, or the far machine runs a build too old to ask.
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
    /// update, for a list with no `gglib_id`; `Unavailable` when the
    /// request did not get through in three seconds, or the list does not
    /// read for another reason.
    pub async fn models(&self) -> Result<ModelsResponse, FarError> {
        let request = self.bounded.get(self.url("/models")).timeout(LIST_TIMEOUT);
        let body = accepted(self.send(request).await?).await?;
        serde_json::from_slice(&body).map_err(|e| {
            if e.to_string().contains("`gglib_id`") {
                FarError::Failed(GuiError::Conflict(OLDER_FAR.to_owned()))
            } else {
                unreadable(&e)
            }
        })
    }

    /// `GET /v1/models/{identifier}/detail`: one model, resolved there as a
    /// turn sent with `identifier` would be. The identifier travels as one
    /// path segment, so a name holding `/` or `:` arrives whole.
    ///
    /// # Errors
    ///
    /// [`FarError::Refused`] for a far refusal that names its code;
    /// `Conflict`, asking for an update, for a 404 that names none, which is a
    /// build with no such route; `Unavailable` as for [`FarProxy::models`].
    pub async fn lookup(&self, identifier: &str) -> Result<ModelLookup, FarError> {
        let path = format!("/models/{}/detail", path_segment(identifier));
        let answer = self.send(self.bounded.get(self.url(&path))).await?;
        match accepted(answer).await {
            Err(FarError::Refused { status, body, .. })
                if status == StatusCode::NOT_FOUND && !names_a_code(&body) =>
            {
                Err(FarError::Failed(GuiError::Conflict(OLDER_FAR.to_owned())))
            }
            read => decode(&read?),
        }
    }

    /// `POST /v1/models/{identifier}/load` with `{num_ctx}`: have the model
    /// resident now, so the first turn does not wait.
    ///
    /// # Errors
    ///
    /// [`FarError::Refused`] for a far refusal; `Unavailable` as for
    /// [`FarProxy::models`].
    pub async fn load(
        &self,
        identifier: &str,
        num_ctx: Option<u64>,
    ) -> Result<LoadResponse, FarError> {
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
    /// proxy is not running under a daemon, `Unavailable` when the request
    /// did not get through.
    pub async fn shutdown(&self) -> Result<(), GuiError> {
        let request = self
            .bounded
            .post(self.url("/proxy/shutdown"))
            .json(&serde_json::json!({ "confirm": "shutdown" }));
        match self.send(request).await?.status() {
            s if s.is_success() => {
                info!("the remote daemon accepted the shutdown");
                Ok(())
            }
            // Deliberately not "its API key has changed": the key sent is one
            // that machine issued, so a refusal means it no longer admits this
            // device's key, whether because it stopped trusting the device or
            // a rotation there has not reached the tunnel yet. That is the
            // claim the chat path's refusal makes too.
            StatusCode::UNAUTHORIZED => Err(GuiError::ValidationFailed(format!(
                "the remote machine {} is not admitting this device's key — it has either \
                 stopped trusting this device, or a key rotation there is still reaching the \
                 tunnel, which clears itself within a few seconds. If waiting does not fix it, \
                 pair again with a fresh `gglib remote invite` there",
                self.fingerprint
            ))),
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
fn unreadable(e: &serde_json::Error) -> FarError {
    FarError::Failed(GuiError::Unavailable(format!(
        "the other machine answered something this build cannot read: {e}"
    )))
}

/// Whether a refusal's body is the proxy's `{"error": {"code": …}}`. Every
/// refusal a gglib proxy writes names one; axum's own 404 for a route that
/// does not exist is empty.
fn names_a_code(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/code").and_then(|c| c.as_str()).map(drop))
        .is_some()
}
