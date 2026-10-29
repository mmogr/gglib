//! The dashboard publisher runs only when the serve asks for it.
//!
//! `GET /v1/proxy/status/stream` opens with the current snapshot, then gets one
//! more each time the publisher ticks, once a second. Served with
//! `dashboard: false` the stream opens the same way and nothing follows it;
//! served with the publisher on, the next snapshot arrives within a tick. Real
//! time, as the publisher's own test in `dashboard_tests.rs` uses: a paused
//! clock does not promise the woken publisher has sent before the test looks.

mod fixtures;

use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_proxy::ServeConfig;

use fixtures::spawn::{defaults, spawn};

/// The publisher's one-second tick, twice over and a half.
const WATCH_FOR: Duration = Duration::from_millis(2500);

/// How many snapshots the status stream of a proxy served with `dashboard`
/// carries in its first [`WATCH_FOR`], the opening one included.
async fn snapshots_seen(dashboard: bool) -> usize {
    let proxy = spawn(ServeConfig {
        dashboard,
        ..defaults().await
    })
    .await;
    let response = reqwest::Client::new()
        .get(format!("{}/v1/proxy/status/stream", proxy.base))
        .send()
        .await
        .expect("the stream opens");
    assert!(response.status().is_success());

    let mut stream = response.bytes_stream();
    let mut text = String::new();
    let _ = tokio::time::timeout(WATCH_FOR, async {
        while let Some(chunk) = stream.next().await {
            text.push_str(&String::from_utf8_lossy(&chunk.expect("the stream reads")));
        }
    })
    .await;
    proxy.cancel.cancel();

    text.lines()
        .filter(|line| line.starts_with("data:"))
        .count()
}

/// With the publisher off, the stream carries its opening snapshot and no
/// other.
#[tokio::test]
async fn with_the_dashboard_off_no_snapshot_follows_the_first() {
    assert_eq!(snapshots_seen(false).await, 1);
}

/// With it on, a published snapshot follows the opening one, so the test
/// above would see one if the publisher ran.
#[tokio::test]
async fn with_the_dashboard_on_published_snapshots_follow() {
    assert!(snapshots_seen(true).await >= 2);
}
