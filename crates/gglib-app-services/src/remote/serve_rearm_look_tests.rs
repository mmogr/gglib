//! Tests for one look of the daemon's follower, called directly rather than
//! through its poll, against a real arm: the two refusals a re-arm makes with
//! nothing in front of it, and a race it runs — a proxy that leaves while the
//! tunnel it is putting back binds. `serve_rearm_disable_tests.rs` has the
//! races with a `disable`, on the store double here.
//!
//! A race is held in place by a settings store that parks a read after it
//! has read and before it answers — here `arm`'s read for the key — until
//! the test has done the thing that races it. So none needs a clock.
//!
//! **Everything fallible is asserted after the cleanup**, for
//! `serve_rearm_tests.rs`'s reason.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::ports::{AppEventEmitter, RepositoryError, SettingsRepository};
use gglib_core::services::AppCore;
use gglib_core::{RemoteServe, Settings, SettingsUpdate};
use gglib_db::{CoreFactory, setup_test_database};
use tokio::sync::{MutexGuard, Notify};
use tokio::time::timeout;

use super::*;
use crate::proxy::ProxyOps;
use crate::remote::RemoteGateway;
use crate::remote::enable_tests::free_port;
use crate::remote::serve_watch_tests::{arming, offline, ops_with_key};
use crate::test_support::{RecordingEmitter, test_core_and_proxy_over};
use crate::test_support_remote::scratch_device_keys;

/// The state `enable` leaves behind, which is what a re-arm arms from.
pub(super) async fn switched_on(core: &AppCore) {
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            remote_serve: Some(Some(RemoteServe {
                allow_mcp: false,
                relay: None,
                discovery: false,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is recorded");
}

/// A settings store that parks up to two reads, each until the test opens it.
/// A write through the trait's `modify` reads first, so a write can be parked
/// too, before it has written anything.
pub(super) struct Parked {
    store: Arc<dyn SettingsRepository>,
    /// Reads since [`Parked::park`].
    reads: AtomicUsize,
    /// One gate for each read parked.
    pub(super) gates: [Gate; 2],
}

/// Where one read waits.
#[derive(Default)]
pub(super) struct Gate {
    /// Which read to park, counted from [`Parked::park`]; none while zero.
    nth: AtomicUsize,
    /// Notified when the parked read has read.
    reached: Notify,
    /// Notified by the test to let it answer.
    opened: Notify,
}

#[async_trait]
impl SettingsRepository for Parked {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let read = self.store.load().await;
        let nth = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        for gate in &self.gates {
            if gate.nth.load(Ordering::SeqCst) == nth {
                gate.reached.notify_one();
                gate.opened.notified().await;
            }
        }
        read
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        self.store.save(settings).await
    }
}

impl Parked {
    /// Park the reads these name, counted from now; a zero parks nothing.
    pub(super) fn park(&self, nth: [usize; 2]) {
        self.reads.store(0, Ordering::SeqCst);
        for (gate, nth) in self.gates.iter().zip(nth) {
            gate.nth.store(nth, Ordering::SeqCst);
        }
    }
}

impl Gate {
    /// Whether the read parked here has read, within a poll: it is the next
    /// thing its caller does, and far quicker than that.
    pub(super) async fn reached(&self) -> bool {
        timeout(PROXY_POLL, self.reached.notified()).await.is_ok()
    }

    /// Let the read parked here answer.
    pub(super) fn open(&self) {
        self.opened.notify_one();
    }
}

/// `ops_with_key`'s world over a [`Parked`] store, with the switch on and the
/// proxy running, so a look goes as far as the reservation. The last element
/// is the arming guard, as there.
pub(super) async fn ops_over_a_parked_store() -> (
    Arc<AppCore>,
    Arc<Parked>,
    Arc<ProxyOps>,
    Arc<RemoteOps>,
    MutexGuard<'static, ()>,
) {
    let pool = setup_test_database().await.expect("in-memory DB");
    let mut repos = CoreFactory::build_repos(pool);
    let parked = Arc::new(Parked {
        store: Arc::clone(&repos.settings),
        reads: AtomicUsize::new(0),
        gates: Default::default(),
    });
    repos.settings = Arc::clone(&parked) as Arc<dyn SettingsRepository>;
    let (core, proxy) = test_core_and_proxy_over(&repos);
    core.settings()
        .update(SettingsUpdate {
            proxy_port: Some(Some(free_port().await)),
            proxy_api_key: Some(Some("already-enforced".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");
    switched_on(&core).await;
    let events: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(RemoteGateway::new(Arc::clone(&events)));
    let ops = RemoteOps::new(
        Arc::clone(&proxy),
        Arc::clone(&core),
        gateway,
        events,
        Some(scratch_device_keys()),
    );
    let arming = arming().await;
    proxy.ensure_running().await.expect("the proxy starts");
    (core, parked, proxy, Arc::new(ops), arming)
}

/// A look, spawned.
pub(super) fn looking(ops: &Arc<RemoteOps>) -> tokio::task::JoinHandle<Rearm> {
    let ops = Arc::clone(ops);
    tokio::spawn(async move { ops.rearm(&CancellationToken::new()).await })
}

/// The proxy leaves while the tunnel it was going to front is binding. The
/// bind is refused, and the tunnel is still owed: the next look puts it back
/// once the proxy runs again, where giving up on it would leave it down.
#[tokio::test]
async fn a_proxy_that_leaves_while_the_tunnel_binds_is_still_waited_for() {
    let (_core, parked, proxy, ops, _arming) = ops_over_a_parked_store().await;
    // The look's read of the switch is the first, and `arm`'s read for the
    // key the second: after the address was taken, before the bind.
    parked.park([2, 0]);
    let look = looking(&ops);
    let reached = parked.gates[0].reached().await;
    proxy.stop().await.expect("the proxy stops");
    parked.gates[0].open();
    let decision = look.await.expect("the look");
    let up = ops.status().await.enabled;

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(reached, "the look never reached the key");
    assert_eq!(decision, Rearm::ProxyNotRunning);
    assert!(!up, "a tunnel was left fronting a proxy that had gone");
}

/// A re-arm asked for against a stopped proxy is an error, and the proxy is
/// still stopped after it: the refusal the follower reports as "not running",
/// with nothing in front of it.
#[tokio::test]
async fn a_rearm_against_a_stopped_proxy_refuses_and_leaves_it_stopped() {
    // This fixture starts nothing.
    let (_core, proxy, _events, ops, _arming) = ops_with_key().await;
    let caller = Caller::Rearm(CancellationToken::new());
    let refused = ops
        .turn_on(offline(), caller, ops.disables.subscribe())
        .await;
    let status = proxy.status().await;

    let _ = ops.shut_down().await;
    let _ = proxy.stop().await;

    assert!(
        matches!(refused, Err(GuiError::Conflict(ref m)) if m == PROXY_NOT_RUNNING),
        "{refused:?}"
    );
    assert_eq!(status, ProxyStatus::Stopped, "the re-arm started the proxy");
}

/// A look already under way when the daemon's token is cancelled reaches the
/// slot after it, and gives the slot back: it reads the token under the same
/// hold as its reservation, so it cannot arm into the teardown.
#[tokio::test]
async fn a_rearm_that_reaches_the_slot_after_the_token_arms_nothing() {
    let (_core, _parked, proxy, ops, _arming) = ops_over_a_parked_store().await;
    let shutdown = CancellationToken::new();
    shutdown.cancel();

    let decision = ops.rearm(&shutdown).await;
    let up = ops.status().await.enabled;

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert_eq!(decision, Rearm::Ended);
    assert!(!up, "a re-arm armed after the daemon's token was cancelled");
}
