//! The far machine's runs this device may see: the list, a stop, and a
//! run's events as they come.
//!
//! The events are the one stream here, read through the streaming client.
//! Each call is made only to a machine that says it shares its runs
//! (`far_said.rs`).

use gglib_core::domain::runs::is_run_id;
use gglib_proxy::models::Feature;

use super::FarProxy;
use crate::error::GuiError;

impl FarProxy {
    /// `GET /v1/gglib/runs`: the runs this device may see there.
    ///
    /// # Errors
    ///
    /// `Conflict` when that machine does not share its runs, or runs a gglib
    /// too old to say ([`FarProxy::offers`]); `Unavailable` when the request
    /// did not get through.
    pub async fn list_runs(&self) -> Result<reqwest::Response, GuiError> {
        self.offers(&[Feature::Runs]).await?;
        self.send(self.bounded.get(self.url("/gglib/runs"))).await
    }

    /// `POST /v1/gglib/runs/{run_id}/cancel`.
    ///
    /// # Errors
    ///
    /// `ValidationFailed` for a run id the hub would not accept, before
    /// anything is sent; otherwise as [`FarProxy::list_runs`].
    pub async fn cancel_run(&self, run_id: &str) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}/cancel", run_path(run_id)?);
        self.offers(&[Feature::Runs]).await?;
        self.send(self.bounded.post(self.url(&path))).await
    }

    /// `GET /v1/gglib/runs/{run_id}/events?after={after}`, answered as soon as its
    /// head arrives: the body is the stream, read as the run goes.
    ///
    /// # Errors
    ///
    /// As [`FarProxy::cancel_run`].
    pub async fn run_events(
        &self,
        run_id: &str,
        after: u32,
    ) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}/events?after={after}", run_path(run_id)?);
        self.offers(&[Feature::Runs]).await?;
        let request = self
            .streaming
            .get(self.url(&path))
            .header(reqwest::header::ACCEPT, "text/event-stream");
        self.send(request).await
    }
}

/// `/gglib/runs/{run_id}`, for an id the hub accepts: never a path a caller built.
pub(super) fn run_path(run_id: &str) -> Result<String, GuiError> {
    if !is_run_id(run_id) {
        return Err(GuiError::ValidationFailed(
            "a run id is 1 to 64 of A-Z, a-z, 0-9, '_' and '-'".to_owned(),
        ));
    }
    Ok(format!("/gglib/runs/{run_id}"))
}

#[cfg(test)]
#[path = "far_runs_tests.rs"]
mod far_runs_tests;
