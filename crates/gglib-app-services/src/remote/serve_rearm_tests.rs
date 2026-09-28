//! Tests for the daemon's follower ([#1040]): a tunnel that went down with
//! its proxy comes back when the proxy does, and in every other case the
//! follower says why it did not.
//!
//! On `serve_watch_tests.rs`'s fixture: a real proxy on a free port, and a
//! `modelpipe::serve` that binds without reaching the network, so a re-arm
//! really arms. Each test asserts what the follower *decided*, from the value
//! it reports, rather than that a tunnel has not come back yet — which a test
//! checking before the next look would see under every mutation that
//! matters. `serve_rearm_look_tests.rs` has the tests that call one look
//! directly, `serve_rearm_disable_tests.rs` a look's races with a `disable`,
//! `serve_rearm_once_tests.rs` the follower saying each thing once, and
//! `serve_rearm_daemon_tests.rs` the follower as the daemon starts it.
//!
//! **Everything fallible is asserted after the cleanup**: a tunnel wrongly
//! put back holds the shared identity file, and it is taken down before
//! anything can panic.
//!
//! [#1040]: https://github.com/mmogr/gglib/issues/1040

use std::sync::Arc;
use std::time::Duration;

use gglib_core::SettingsUpdate;
use gglib_core::services::AppCore;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::*;
use crate::proxy::ProxyOps;
use crate::remote::serve_watch_tests::{offline, ops_with_key};
use crate::remote::{DRAIN, WAIT_ONLINE};

/// The longest one look takes to be reported once it is owed: the rest of
/// the follower's poll, then an arm, which waits up to `WAIT_ONLINE` for a
/// relay and, when the proxy leaves while it binds, `DRAIN` for the listener.
pub(super) fn a_look() -> Duration {
    PROXY_POLL + WAIT_ONLINE + DRAIN
}

/// What the switch reads now.
async fn switch(core: &AppCore) -> Option<bool> {
    core.settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.remote_enabled)
}

/// The follower over `ops`, as a test sees it: what it decides, on a
/// channel, and the token that ends it.
pub(super) struct Follower {
    decisions: UnboundedReceiver<Rearm>,
    shutdown: CancellationToken,
    task: JoinHandle<()>,
}

/// Follow from `lost`, subscribed before anything it is to hear of was lost,
/// as `resume_and_follow` subscribes before its resume.
pub(super) fn follow(ops: &Arc<RemoteOps>, lost: watch::Receiver<u64>) -> Follower {
    let shutdown = CancellationToken::new();
    let (sent, decisions) = unbounded_channel();
    let task = tokio::spawn({
        let ops = Arc::clone(ops);
        let shutdown = shutdown.clone();
        async move {
            ops.follow(lost, &shutdown, |decision| {
                let _ = sent.send(decision);
            })
            .await;
        }
    });
    Follower {
        decisions,
        shutdown,
        task,
    }
}

impl Follower {
    /// The next decision, or `None` when none came within a look.
    pub(super) async fn next(&mut self) -> Option<Rearm> {
        timeout(a_look(), self.decisions.recv())
            .await
            .ok()
            .flatten()
    }

    /// The first decision after the proxy was started again that is not
    /// "the proxy is not running". A look that began before the start can
    /// still say that; the one a poll after it cannot.
    async fn settled(&mut self) -> Option<Rearm> {
        timeout(a_look() + PROXY_POLL, async {
            loop {
                match self.decisions.recv().await {
                    Some(Rearm::ProxyNotRunning) => {}
                    other => return other,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    /// Cancel the token, and whether the follower then ended within a poll.
    pub(super) async fn end(&mut self) -> bool {
        self.shutdown.cancel();
        timeout(PROXY_POLL, &mut self.task).await.is_ok()
    }
}

/// Stop the proxy, and whether the tunnel then went down with it: `lost`
/// moves once the watcher has taken the tunnel down and drained it, which is
/// at most a poll and a drain after the stop.
pub(super) async fn stop_and_lose(proxy: &ProxyOps, ops: &RemoteOps) -> bool {
    let mut lost = ops.lost_with_proxy.subscribe();
    proxy.stop().await.expect("the test proxy stops");
    timeout(PROXY_POLL + DRAIN, lost.changed())
        .await
        .is_ok_and(|changed| changed.is_ok())
}

/// What the issue was about: the proxy stopped and started again, and remote
/// access left switched on with nothing bound until the daemon restarted.
/// The follower puts the tunnel back from the switch, with no code, on the
/// same endpoint key and so on the same ticket.
#[tokio::test]
async fn a_proxy_that_comes_back_brings_the_tunnel_back_on_the_same_ticket() {
    let (core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let mut follower = follow(&ops, ops.lost_with_proxy.subscribe());
    ops.enable(offline()).await.expect("the tunnel comes up");
    let before = ops.status().await.ticket_fingerprint;

    let lost = stop_and_lose(&proxy, &ops).await;
    proxy
        .ensure_running()
        .await
        .expect("the proxy starts again");
    let decision = follower.settled().await;
    let after = ops.status().await;
    let switch = switch(&core).await;

    follower.end().await;
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(lost, "the tunnel never went down with the proxy");
    assert_eq!(decision, Some(Rearm::Armed));
    assert!(after.enabled, "the tunnel is serving again");
    assert!(!after.pairing_active, "put back with no code");
    assert!(before.is_some(), "the tunnel had a ticket to begin with");
    assert_eq!(after.ticket_fingerprint, before, "the same ticket");
    assert_eq!(switch, Some(true), "the switch is still on");
}

/// `disable` is the off switch, and a proxy coming back does not undo it.
/// Run while the proxy is down, it finds nothing bound to take down, so the
/// switch it wrote off is what the follower has to read.
#[tokio::test]
async fn a_disable_while_the_proxy_is_down_keeps_the_tunnel_down_when_it_comes_back() {
    let (core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let mut follower = follow(&ops, ops.lost_with_proxy.subscribe());
    ops.enable(offline()).await.expect("the tunnel comes up");

    let lost = stop_and_lose(&proxy, &ops).await;
    // "Not enabled", since nothing is bound; the switch is written off first.
    let _ = ops.disable().await;
    proxy
        .ensure_running()
        .await
        .expect("the proxy starts again");
    let decision = follower.settled().await;
    let up = ops.status().await.enabled;
    let switch = switch(&core).await;

    follower.end().await;
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(lost, "the tunnel never went down with the proxy");
    assert_eq!(decision, Some(Rearm::SwitchOff));
    assert!(!up, "the tunnel came back after a disable");
    assert_eq!(switch, Some(false), "the switch is still off");
}

/// A stopped proxy is a person's answer. The follower says once that it is
/// waiting, does not start it, and puts the tunnel back once someone does;
/// when the proxy stops again, it says it is waiting again.
#[tokio::test]
async fn a_proxy_that_stays_stopped_is_waited_for_and_not_started() {
    let (_core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let mut follower = follow(&ops, ops.lost_with_proxy.subscribe());
    ops.enable(offline()).await.expect("the tunnel comes up");

    let lost = stop_and_lose(&proxy, &ops).await;
    let waiting = follower.next().await;
    // Polls at which the follower looks again, and would say it again.
    tokio::time::sleep(PROXY_POLL * 2).await;
    let status = proxy.status().await;
    proxy.ensure_running().await.expect("the proxy starts");
    let decision = follower.next().await;
    let lost_again = stop_and_lose(&proxy, &ops).await;
    let waiting_again = follower.next().await;

    follower.end().await;
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(lost, "the tunnel never went down with the proxy");
    assert_eq!(waiting, Some(Rearm::ProxyNotRunning));
    assert_eq!(status, ProxyStatus::Stopped, "the proxy was started");
    assert_eq!(decision, Some(Rearm::Armed), "the next thing it said");
    assert!(lost_again, "the tunnel put back never went down");
    assert_eq!(waiting_again, Some(Rearm::ProxyNotRunning), "unsaid");
}

/// The daemon's teardown empties the serve slot with the proxy still up, so
/// the follower has to be gone by then. It ends on the shutdown token and
/// says so. Nothing is armed after that because its task has returned,
/// which `ended` asserts, and a task that has returned arms nothing; a look
/// that reaches the slot after the token is turned away there as well
/// (`serve_rearm_look_tests.rs`).
#[tokio::test]
async fn the_follower_ends_on_the_shutdown_token_and_nothing_is_armed_after() {
    let (_core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let mut follower = follow(&ops, ops.lost_with_proxy.subscribe());
    ops.enable(offline()).await.expect("the tunnel comes up");

    let ended = follower.end().await;
    // Its sender went with it, so this is everything it reported.
    let mut reported = Vec::new();
    while let Ok(decision) = follower.decisions.try_recv() {
        reported.push(decision);
    }

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(ended, "the follower outlived its token");
    assert_eq!(reported, [Rearm::Ended], "nothing was owed a look");
}

/// The documented way to turn local authentication off — unset the key, then
/// stop the proxy and start it — leaves the state in which arming mints one.
/// A re-arm does not: that key would lock the local proxy the person had just
/// opened, with nobody there to be told.
#[tokio::test]
async fn a_proxy_restarted_without_its_key_is_left_open_and_the_tunnel_down() {
    let (core, proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let mut follower = follow(&ops, ops.lost_with_proxy.subscribe());
    ops.enable(offline()).await.expect("the tunnel comes up");
    core.settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(None),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the key is unset");

    let lost = stop_and_lose(&proxy, &ops).await;
    let address = proxy
        .ensure_running()
        .await
        .expect("the proxy starts again");
    let decision = follower.settled().await;
    let stored = core
        .settings()
        .get()
        .await
        .map(|s| s.proxy_api_key.is_some());
    let answered = gglib_proxy::loopback::client()
        .get(format!("http://{address}/v1/proxy/status"))
        .send()
        .await
        .map(|response| response.status());

    follower.end().await;
    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(lost, "the tunnel never went down with the proxy");
    assert_eq!(decision, Some(Rearm::WouldMint));
    assert_eq!(stored.ok(), Some(false), "a key was stored");
    assert!(
        answered.as_ref().is_ok_and(reqwest::StatusCode::is_success),
        "the local proxy no longer admits a request with no key: {answered:?}"
    );
}
