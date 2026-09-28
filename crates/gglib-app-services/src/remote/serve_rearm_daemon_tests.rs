//! The follower as the daemon starts it ([#1040]): `resume_and_follow`, which
//! the daemon spawns with its shutdown token. The other `serve_rearm` tests
//! call `follow`, or one look, with a token and a reporter of their own, so
//! they pass whether or not this entry point follows at all, and whichever
//! token it hands the follower. This test goes through it.
//!
//! The daemon only logs what the follower decides, so this watches the event
//! each arm sends instead: one from the startup resume, and one from the
//! follower once the proxy runs again.
//!
//! **Everything fallible is asserted after the cleanup**, for
//! `serve_rearm_tests.rs`'s reason.
//!
//! [#1040]: https://github.com/mmogr/gglib/issues/1040

use std::sync::Arc;
use std::time::Duration;

use gglib_core::events::AppEvent;
use tokio::time::timeout;

use super::serve_rearm_look_tests::switched_on;
use super::serve_rearm_tests::{a_look, stop_and_lose};
use super::*;
use crate::remote::WAIT_OUT_RESUME;
use crate::remote::enable_tests::Recording;
use crate::remote::serve_watch_tests::ops_with_key;

/// The ticket fingerprint of each tunnel armed so far, in order.
fn armed(events: &Recording) -> Vec<String> {
    events
        .0
        .lock()
        .expect("the recorded events")
        .iter()
        .filter_map(|event| match event {
            AppEvent::RemoteEnabled { ticket_fingerprint } => Some(ticket_fingerprint.clone()),
            _ => None,
        })
        .collect()
}

/// [`armed`], once it lists `n` tunnels or `within` has passed.
async fn until_armed(events: &Recording, n: usize, within: Duration) -> Vec<String> {
    let _ = timeout(within, async {
        while armed(events).len() < n {
            tokio::task::yield_now().await;
        }
    })
    .await;
    armed(events)
}

/// A daemon that starts with remote access switched on: its startup resume
/// arms the tunnel; the proxy is stopped and started again; and the same
/// task, following, puts the tunnel back on the same ticket. Cancelling the
/// daemon's token then ends that task.
#[tokio::test]
async fn the_task_the_daemon_spawns_puts_the_tunnel_back_and_ends_on_its_token() {
    let (core, proxy, events, ops, _arming) = ops_with_key().await;
    switched_on(&core).await;
    let ops = Arc::new(ops);
    let shutdown = CancellationToken::new();
    let mut task = tokio::spawn({
        let ops = Arc::clone(&ops);
        let shutdown = shutdown.clone();
        async move { ops.resume_and_follow(shutdown).await }
    });

    // The resume starts the proxy, which this fixture leaves stopped.
    let resumed = until_armed(&events, 1, WAIT_OUT_RESUME).await;
    let lost = stop_and_lose(&proxy, &ops).await;
    proxy
        .ensure_running()
        .await
        .expect("the proxy starts again");
    let rearmed = until_armed(&events, 2, a_look() + PROXY_POLL).await;
    let serving = ops.status().await.enabled;
    let following = !task.is_finished();
    shutdown.cancel();
    let ended = timeout(PROXY_POLL, &mut task).await.is_ok();

    task.abort();
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert_eq!(resumed.len(), 1, "the startup resume armed no tunnel");
    assert!(lost, "the tunnel never went down with the proxy");
    assert_eq!(rearmed.len(), 2, "the tunnel was not put back");
    assert_eq!(rearmed[1], rearmed[0], "the same ticket");
    assert!(serving, "the tunnel is serving again");
    assert!(following, "the task had returned before its token");
    assert!(ended, "the task outlived the daemon's token");
}
