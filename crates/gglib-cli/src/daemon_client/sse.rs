//! Minimal SSE consumption for daemon streams.
//!
//! The daemon streams long-running operations (benchmarks, events) as
//! `text/event-stream` frames whose `data:` payload is one JSON value.
//! [`drain_events`] does the framing; [`stream_json`] drives a whole POST
//! stream, handing each decoded value to the caller.

use anyhow::{Context, Result};
use futures_util::StreamExt as _;

/// Extract complete SSE `data:` payloads from a growing byte buffer.
///
/// Splits on the blank-line event terminator (`"\n\n"`), joining any
/// `data:`-prefixed lines within an event (gglib always emits single-line
/// JSON, but multi-line `data:` framing is handled per spec anyway). Comment
/// lines (leading `:`, used for SSE keep-alives) and events with no `data:`
/// line are silently skipped. Any trailing partial event is left in `buffer`
/// for the next call once more bytes arrive.
pub(crate) fn drain_events(buffer: &mut String) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let event: String = buffer.drain(..idx + 2).collect();
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if !data.is_empty() {
            payloads.push(data);
        }
    }
    payloads
}

/// POST `body` to `url` and hand every streamed JSON event to `on_event`.
///
/// Runs until the server closes the stream. Dropping the future (Ctrl-C on
/// the caller) drops the response, which is exactly the disconnect signal
/// the daemon's benchmark guard cancels on.
pub(crate) async fn stream_json<T, B>(
    client: &reqwest::Client,
    url: &str,
    body: &B,
    mut on_event: impl FnMut(T),
) -> Result<()>
where
    T: serde::de::DeserializeOwned,
    B: serde::Serialize + ?Sized,
{
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .with_context(|| format!("connecting to {url}"))?;

    anyhow::ensure!(
        response.status().is_success(),
        "daemon answered {} for {url}",
        response.status()
    );

    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();
    while let Some(chunk) = byte_stream.next().await {
        let chunk = chunk.context("reading event stream")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        for payload in drain_events(&mut buffer) {
            match serde_json::from_str::<T>(&payload) {
                Ok(event) => on_event(event),
                Err(e) => tracing::warn!("skipping undecodable stream event: {e}"),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_extracts_single_complete_event() {
        let mut buffer = String::from("data: {\"a\":1}\n\n");
        assert_eq!(drain_events(&mut buffer), vec!["{\"a\":1}"]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_leaves_partial_event_buffered() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"b\"");
        assert_eq!(drain_events(&mut buffer), vec!["{\"a\":1}"]);
        assert_eq!(buffer, "data: {\"b\"");
    }

    #[test]
    fn drain_skips_keepalive_comments() {
        let mut buffer = String::from(": keep-alive\n\ndata: {\"a\":1}\n\n");
        assert_eq!(drain_events(&mut buffer), vec!["{\"a\":1}"]);
    }
}
