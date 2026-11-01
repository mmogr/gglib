//! The far machine's runs this device may see: the list, a stop, and a
//! run's events as they come.
//!
//! The events are the one stream here, read through the streaming client.

use gglib_core::domain::runs::is_run_id;

use super::FarProxy;
use crate::error::GuiError;

impl FarProxy {
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
    /// As [`FarProxy::add_turn`].
    pub async fn cancel_run(&self, run_id: &str) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}/cancel", run_path(run_id)?);
        self.send(self.bounded.post(self.url(&path))).await
    }

    /// `GET /v1/runs/{run_id}/events?after={after}`, answered as soon as its
    /// head arrives: the body is the stream, read as the run goes.
    ///
    /// # Errors
    ///
    /// As [`FarProxy::add_turn`].
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
}

/// `/runs/{run_id}`, for an id the hub accepts: never a path a caller built.
pub(super) fn run_path(run_id: &str) -> Result<String, GuiError> {
    if !is_run_id(run_id) {
        return Err(GuiError::ValidationFailed(
            "a run id is 1 to 64 of A-Z, a-z, 0-9, '_' and '-'".to_owned(),
        ));
    }
    Ok(format!("/runs/{run_id}"))
}

#[cfg(test)]
#[path = "far_runs_tests.rs"]
mod far_runs_tests;
