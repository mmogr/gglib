//! The daemon's runs, on a [`DaemonHandle`]: replies it owns until they end.
//!
//! Every id reaching here has passed `is_run_id`, since it is interpolated
//! into a path.

use std::ops::ControlFlow;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use gglib_core::domain::runs::{RunInfo, RunList};
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
    let mut pending = Vec::new();
    let mut buffer = String::new();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.context("reading the run's events")?;
        // Decoded only up to the last whole event, so a character split
        // across two reads is decoded whole.
        pending.extend_from_slice(chunk.as_ref());
        let Some(cut) = pending.windows(2).rposition(|w| w == b"\n\n") else {
            continue;
        };
        let whole: Vec<u8> = pending.drain(..cut + 2).collect();
        buffer.push_str(&String::from_utf8_lossy(&whole));
        for item in drain_items(&mut buffer)? {
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

/// Take every complete event out of `buffer`: `id:` and `data:` make a
/// frame, `event: run` the run's end. Keep-alive comments are skipped.
pub(crate) fn drain_items(buffer: &mut String) -> Result<Vec<RunItem>> {
    let mut items = Vec::new();
    while let Some(end) = buffer.find("\n\n") {
        let event: String = buffer.drain(..end + 2).collect();
        let (mut id, mut name, mut data) = (None, None, Vec::new());
        for line in event.lines() {
            if let Some(rest) = line.strip_prefix("id:") {
                id = Some(rest.trim().to_owned());
            } else if let Some(rest) = line.strip_prefix("event:") {
                name = Some(rest.trim().to_owned());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data.push(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if data.is_empty() {
            continue;
        }
        let data = data.join("\n");
        if name.as_deref() == Some("run") {
            let info = serde_json::from_str(&data).context("reading the run's final state")?;
            items.push(RunItem::End(info));
        } else {
            let seq = id
                .and_then(|id| id.parse().ok())
                .context("an event without its number")?;
            items.push(RunItem::Frame { seq, data });
        }
    }
    Ok(items)
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod runs_tests;
