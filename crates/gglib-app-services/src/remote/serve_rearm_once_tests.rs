//! The follower saying each thing once ([#1040]): a lost tunnel is looked at
//! again only while its proxy is not running, so any other decision is the
//! last the daemon logs for that tunnel, rather than a line at every
//! [`PROXY_POLL`]. `serve_rearm_tests.rs` holds a run of "not running" to one
//! line; this holds the other decisions to theirs.
//!
//! On `serve_rearm_tests.rs`'s fixture. The test waits out two polls at the
//! point where a repeat would come, whatever the phase of the follower's
//! tick, and then asserts the decision that has to come next, which a repeat
//! would arrive in front of.
//!
//! **Everything fallible is asserted after the cleanup**, for
//! `serve_rearm_tests.rs`'s reason.
//!
//! [#1040]: https://github.com/mmogr/gglib/issues/1040

use std::sync::Arc;

use super::serve_rearm_tests::{follow, stop_and_lose};
use super::*;
use crate::remote::serve_watch_tests::{offline, ops_with_key};

/// A person's `enable` that puts the tunnel back before the follower looks
/// leaves the follower a look that finds the slot taken. That is reported
/// once, as a failure, and not again at every poll after it: the next thing
/// reported is about the next tunnel lost.
#[tokio::test]
async fn a_look_that_finds_the_tunnel_back_already_is_reported_once() {
    let (_core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let lost = ops.lost_with_proxy.subscribe();
    ops.enable(offline()).await.expect("the tunnel comes up");
    let went = stop_and_lose(&proxy, &ops).await;
    // It starts the proxy and arms before the follower exists, so the
    // follower cannot get there first.
    let back = ops.enable(offline()).await.is_ok();
    let mut follower = follow(&ops, lost);
    let first = follower.next().await;
    tokio::time::sleep(PROXY_POLL * 2).await;
    let went_again = stop_and_lose(&proxy, &ops).await;
    let second = follower.next().await;

    follower.end().await;
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(went, "the tunnel never went down with the proxy");
    assert!(back, "the person's enable did not put it back");
    assert!(
        matches!(&first, Some(Rearm::Failed(m)) if m.contains("already enabled")),
        "{first:?}"
    );
    assert!(went_again, "the tunnel never went down the second time");
    assert_eq!(second, Some(Rearm::ProxyNotRunning), "said again instead");
}
