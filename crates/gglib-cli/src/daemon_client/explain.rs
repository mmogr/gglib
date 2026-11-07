//! A model's launch explained by the daemon, on a [`DaemonHandle`].
//!
//! The daemon plans a Load over the models it has loaded, so its plan says
//! which one a Load would unload first. A one-shot command plans over none.

use std::time::Duration;

use anyhow::{Result, anyhow};
use gglib_app_services::LaunchExplanationDto;

use super::{DaemonHandle, paths};

impl DaemonHandle {
    /// The daemon's explanation of model `model_id`, with `profile` applied
    /// when one is named: `GET /api/models/{id}/explain`.
    ///
    /// A profile nobody configured is the daemon's 400, whose message is
    /// the whole answer, as it is when a one-shot command refuses it. `None`
    /// is its 401: the token this data root holds is not the one it minted,
    /// so it serves another library and has loaded nothing of this one.
    pub(crate) async fn explain_model(
        &self,
        model_id: i64,
        profile: Option<&str>,
    ) -> Result<Option<LaunchExplanationDto>> {
        // Encoded as a path segment is, which a query value decodes from.
        let query = profile.map_or_else(String::new, |name| {
            format!(
                "?profile={}",
                gglib_core::contracts::http::path_segment(name)
            )
        });
        let response = self
            .get(&format!("{}{query}", paths::model_explain_path(model_id)))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => return Ok(None),
            reqwest::StatusCode::BAD_REQUEST => {
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow!(Self::error_message(body)));
            }
            _ => {}
        }
        Ok(Some(Self::expect_ok(response).await?.json().await?))
    }
}
