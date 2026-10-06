//! Minimal SSE consumption for daemon streams.
//!
//! The daemon streams long-running operations (benchmarks, events) as
//! `text/event-stream` frames whose `data:` payload is one JSON value.
//! [`DataFrames`] does the framing, as it does for every stream the CLI
//! reads; [`read_json`] decodes each payload, and [`stream_json`] drives a
//! whole POST stream through it, handing each decoded value to the caller.

use anyhow::{Context, Result};
use futures_util::StreamExt as _;
use gglib_core::sse::DataFrames;

/// POST `body` to `url` and hand every streamed JSON event to `on_event`.
///
/// Runs until the server closes the stream. Dropping the future (Ctrl-C on
/// the caller) drops the response, which is exactly the disconnect signal
/// the daemon's benchmark guard cancels on.
/// `api_key` is the daemon's credential, or `None` when it wants none. It is
/// taken explicitly because this function predates [`DaemonHandle`]'s bearer
/// field and holds a bare client, so it inherits nothing.
///
/// [`DaemonHandle`]: super::DaemonHandle
#[allow(
    clippy::future_not_send,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn stream_json<T, B>(
    client: &reqwest::Client,
    url: &str,
    api_key: Option<&str>,
    body: &B,
    on_event: impl FnMut(T),
) -> Result<()>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    let request = client.post(url).json(body);
    let request = match api_key {
        Some(key) => request.bearer_auth(key),
        None => request,
    };
    let response = request
        .send()
        .await
        .with_context(|| format!("connecting to {url}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("daemon answered 401: {}", super::auth::unauthorized(&body));
    }
    anyhow::ensure!(
        response.status().is_success(),
        "daemon answered {} for {url}",
        response.status()
    );

    read_json(response.bytes_stream(), on_event).await
}

/// Read a stream of JSON events, handing each to `on_event`, until the
/// stream closes. A payload that is not a `T` is skipped.
pub(crate) async fn read_json<T, B, E>(
    mut bytes: impl futures_util::Stream<Item = Result<B, E>> + Unpin,
    mut on_event: impl FnMut(T),
) -> Result<()>
where
    T: serde::de::DeserializeOwned,
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut frames = DataFrames::unbounded();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.context("reading event stream")?;
        for payload in frames.push(chunk.as_ref()) {
            match serde_json::from_str::<T>(&payload) {
                Ok(event) => on_event(event),
                Err(e) => tracing::warn!("skipping undecodable stream event: {e}"),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod sse_tests;
