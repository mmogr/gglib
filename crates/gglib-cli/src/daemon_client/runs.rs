//! The daemon's runs, on a [`DaemonHandle`]: replies it owns until they end.
//!
//! Every id reaching here has passed `is_run_id`, since it is interpolated
//! into a path.

use std::ops::ControlFlow;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use gglib_core::domain::hub_chats::LocalTurn;
use gglib_core::domain::runs::{RunInfo, RunList};
use gglib_core::sse::DataFrames;
use serde_json::Value;

use super::{DaemonHandle, paths};

/// One item of a run's event stream.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunItem {
    /// One logged event.
    Frame { seq: u32, data: String },
    /// The run ended in this state.
    End(RunInfo),
}

impl DaemonHandle {
    /// Start a run with `body` as its chat request.
    pub(crate) async fn run_start(&self, id: &str, body: &Value) -> Result<RunInfo> {
        let response = self
            .request(reqwest::Method::PUT, &paths::run_path(id))
            .json(body)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Start an agent run: `turn` added to one of this machine's chats,
    /// which the daemon answers on the model, tools and limits the chat's
    /// settings save. A turn the daemon refuses before any run is made, a
    /// folder that is not one or a model that cannot read the turn's
    /// images, is an error in the daemon's own words.
    pub(crate) async fn run_turn(&self, id: &str, turn: &LocalTurn) -> Result<RunInfo> {
        let response = self
            .request(reqwest::Method::PUT, &agent_run_path(id))
            .json(turn)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        if response.status().is_client_error()
            && response.status() != reqwest::StatusCode::UNAUTHORIZED
        {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("{}", Self::error_message(body));
        }
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Every run, newest first.
    pub(crate) async fn run_list(&self) -> Result<RunList> {
        let response = self
            .get(paths::RUNS_PATH)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// One run.
    pub(crate) async fn run_get(&self, id: &str) -> Result<RunInfo> {
        let response = self
            .get(&paths::run_path(id))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop a run (idempotent on the daemon side).
    pub(crate) async fn run_cancel(&self, id: &str) -> Result<RunInfo> {
        let response = self
            .post(&paths::run_cancel_path(id))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Read a run's events after `after`, handing each to `on` until it
    /// breaks or the stream ends. No timeout: a reply takes as long as it
    /// takes. Returns the run's final state when the stream reached it.
    pub(crate) async fn run_events(
        &self,
        id: &str,
        after: u32,
        on: impl FnMut(&RunItem) -> ControlFlow<()>,
    ) -> Result<Option<RunInfo>> {
        let response = self.get(&paths::run_events_path(id, after)).send().await?;
        read_events(Self::expect_ok(response).await?.bytes_stream(), on).await
    }
}

/// The path that starts agent run `id`: [`paths::run_path`] with
/// `?kind=agent`, as the chat page sends a turn.
fn agent_run_path(id: &str) -> String {
    format!("{}?kind=agent", paths::run_path(id))
}

/// Read an event stream, handing each item to `on`, until `on` breaks, the
/// run's end arrives, or the stream closes.
pub(crate) async fn read_events<B, E>(
    mut bytes: impl futures_util::Stream<Item = Result<B, E>> + Unpin,
    mut on: impl FnMut(&RunItem) -> ControlFlow<()>,
) -> Result<Option<RunInfo>>
where
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut frames = DataFrames::unbounded();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.context("reading the run's events")?;
        for item in drain_items(&mut frames, chunk.as_ref())? {
            let flow = on(&item);
            if let RunItem::End(info) = item {
                return Ok(Some(info));
            }
            if flow.is_break() {
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Take `chunk`, and return every item it completed: `id:` and `data:` make
/// a frame, `event: run` the run's end. Keep-alive comments are skipped, and
/// so is a tool's `event: preview`, which is never logged, carries no
/// number, and is not shown here.
pub(crate) fn drain_items(frames: &mut DataFrames, chunk: &[u8]) -> Result<Vec<RunItem>> {
    frames
        .push_events(chunk)
        .into_iter()
        .filter(|event| event.name.as_deref() != Some("preview"))
        .map(|event| {
            if event.name.as_deref() == Some("run") {
                let info =
                    serde_json::from_str(&event.data).context("reading the run's final state")?;
                Ok(RunItem::End(info))
            } else {
                let seq = event
                    .id
                    .and_then(|id| id.parse().ok())
                    .context("an event without its number")?;
                Ok(RunItem::Frame {
                    seq,
                    data: event.data,
                })
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod runs_tests;
