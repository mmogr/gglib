//! Tests for a `disable` that lands while a person's `enable` is on its way
//! to arming: the `disable` wins, and the switch it cleared stays off.
//!
//! Its own file rather than more of `enable_wait_tests.rs`, whose fixture
//! may not reach `ensure_running` and whose subject is waiting out the
//! daemon's resume; nothing here involves a resume. These run on
//! `serve_watch_tests.rs`'s fixture instead: a real proxy on a free port and
//! a `modelpipe::serve` that binds without reaching the network, so an
//! `enable` that is not stopped really arms, and the test sees it.
//!
//! The first two tests are the two sides of the reservation. Before it, a
//! `disable` finds nothing to cancel; after it, its write of the switch can
//! land before the `enable`'s. The first holds the serve slot's lock and the
//! second parks the `enable`'s write, so neither needs a clock. The third
//! repeats the first against a counting store, because only the check at the
//! reservation keeps the switch from being written on at all.
//!
//! **Everything fallible is asserted after the cleanup**: an `enable` that
//! wrongly armed holds a live tunnel on the shared identity file, and it is
//! taken down before anything can panic.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ports::{AppEventEmitter, RepositoryError, SettingsRepository};
use gglib_core::services::AppCore;
use gglib_core::{Settings, SettingsUpdate};
use gglib_db::{CoreFactory, setup_test_database};
use tokio::sync::{MutexGuard, Notify};

use super::enable_tests::free_port;
use super::serve_switch::CANCELLED_BY_DISABLE;
use super::serve_watch_tests::{arming, offline, ops_with_key};
use super::*;
use crate::error::GuiError;
use crate::test_support::test_core_and_proxy_over;
use crate::test_support_remote::{RecordingEmitter, scratch_device_keys};

/// Long enough for a call that is not going to wait to have answered.
const ANSWERED: Duration = Duration::from_millis(500);

/// Long enough for the `enable` to start the fixture's proxy, which it does
/// before it reserves anything.
const STARTED: Duration = Duration::from_secs(30);

/// A settings store that parks the first write switching remote access on,
/// until the test opens it.
///
/// `enable` holds no lock between its reservation and that write, so the
/// write itself is the one place a test can stop it there.
struct Gate {
    store: Arc<dyn SettingsRepository>,
    /// Set by the write the gate parks, so that every later write passes.
    spent: AtomicBool,
    /// Every write that stored the switch on, parked or not.
    on_writes: AtomicUsize,
    /// Set once the parked write has been stored.
    landed: AtomicBool,
    /// While set, a write of the switch off after the parked write fails.
    refuse_off: AtomicBool,
    /// Notified when that write arrives.
    reached: Notify,
    /// Notified by the test to let it through.
    opened: Notify,
}

#[async_trait]
impl SettingsRepository for Gate {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        self.store.load().await
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        if settings.remote_enabled == Some(false)
            && self.landed.load(Ordering::SeqCst)
            && self.refuse_off.load(Ordering::SeqCst)
        {
            return Err(RepositoryError::Storage(
                "the store refused the write".to_owned(),
            ));
        }
        let parks = settings.remote_enabled == Some(true) && {
            self.on_writes.fetch_add(1, Ordering::SeqCst);
            !self.spent.swap(true, Ordering::SeqCst)
        };
        if parks {
            self.reached.notify_one();
            self.opened.notified().await;
        }
        let saved = self.store.save(settings).await;
        if parks {
            self.landed.store(true, Ordering::SeqCst);
        }
        saved
    }
}

/// `ops_with_key`, over a settings store that parks the `enable`'s write of
/// the switch when `parks`, and only counts it otherwise. The last element is
/// the arming guard, as there.
async fn ops_with_a_gated_switch(
    parks: bool,
) -> (
    Arc<AppCore>,
    Arc<Gate>,
    Arc<RemoteOps>,
    MutexGuard<'static, ()>,
) {
    let pool = setup_test_database().await.expect("in-memory DB");
    let mut repos = CoreFactory::build_repos(pool);
    let gate = Arc::new(Gate {
        store: Arc::clone(&repos.settings),
        spent: AtomicBool::new(!parks),
        on_writes: AtomicUsize::new(0),
        landed: AtomicBool::new(false),
        refuse_off: AtomicBool::new(false),
        reached: Notify::new(),
        opened: Notify::new(),
    });
    repos.settings = Arc::clone(&gate) as Arc<dyn SettingsRepository>;
    let (core, proxy) = test_core_and_proxy_over(&repos);
    core.settings()
        .update(SettingsUpdate {
            proxy_port: Some(Some(free_port().await)),
            proxy_api_key: Some(Some("already-enforced".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");
    let events: Arc<dyn AppEventEmitter> = Arc::new(RecordingEmitter::default());
    let gateway = Arc::new(RemoteGateway::new(Arc::clone(&events)));
    let ops = RemoteOps::new(
        proxy,
        Arc::clone(&core),
        gateway,
        events,
        Some(scratch_device_keys()),
    );
    (core, gate, Arc::new(ops), arming().await)
}

/// What the switch reads now: `None` when it was never written, or when
/// settings cannot be read.
async fn switch(core: &AppCore) -> Option<bool> {
    core.settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.remote_enabled)
}

/// A `disable` that lands before the `enable` has reserved the slot finds
/// nothing there to cancel. The `enable` sees it when it reserves, gives the
/// slot back and writes nothing, so the switch the `disable` cleared stays
/// off and no tunnel comes up after it.
///
/// The serve slot's lock is held first, so the `enable` parks at its first
/// look at the slot, before it has started the proxy or reserved anything.
/// The `disable` writes the switch before it reaches the slot, so once the
/// switch reads off, it is queued on the same lock behind the `enable`.
#[tokio::test]
async fn a_disable_that_lands_while_a_person_is_arming_leaves_the_switch_off() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let ops = Arc::new(ops);
    let held = Arc::clone(&ops.live).lock_owned().await;
    let mut enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    });
    // Nothing can arm while the lock is held, so this is safe to assert now.
    assert!(
        tokio::time::timeout(ANSWERED, &mut enabling).await.is_err(),
        "the enable answered while the serve slot was held"
    );

    let disabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.disable().await }
    });
    let wrote = tokio::time::timeout(Duration::from_secs(10), async {
        while switch(&core).await != Some(false) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    drop(held);
    let enabled = enabling.await.expect("the enable task");
    let _ = disabling.await;
    let up = ops.status().await.enabled;
    let switched = switch(&core).await;
    // In case the enable armed after all: take it down before judging.
    let _ = ops.disable().await;

    assert!(wrote.is_ok(), "the disable never wrote the switch");
    assert_eq!(
        switched,
        Some(false),
        "the enable switched remote access back on"
    );
    assert!(!up, "the enable armed a tunnel after the disable");
    let Err(GuiError::Conflict(message)) = enabled else {
        panic!("a disable before the reservation cancels the enable: {enabled:?}");
    };
    assert_eq!(message, CANCELLED_BY_DISABLE);
}

/// A `disable` that lands after the `enable` has reserved the slot cancels
/// the arm, but its write of the switch can land first, and the `enable`'s
/// write would then switch remote access back on for the next boot to
/// resume. The `enable` looks again once it has written, finds the
/// `disable`, and writes the switch off.
///
/// The `enable`'s write is parked in the settings store with the slot
/// reserved, and the `disable` runs to the end inside that gap: its write
/// first and the `enable`'s after it, which is the order the race needs.
#[tokio::test]
async fn a_disable_between_the_reservation_and_the_switch_write_is_not_undone_by_it() {
    let (core, gate, ops, _arming) = ops_with_a_gated_switch(true).await;
    let enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    });
    let parked = tokio::time::timeout(STARTED, gate.reached.notified()).await;
    let disabled = ops.disable().await;
    gate.opened.notify_one();
    let enabled = enabling.await.expect("the enable task");
    let up = ops.status().await.enabled;
    let switched = switch(&core).await;
    // In case the enable armed after all: take it down before judging.
    let _ = ops.disable().await;

    assert!(parked.is_ok(), "the enable never wrote the switch");
    assert_eq!(
        switched,
        Some(false),
        "the enable's write undid the disable"
    );
    assert!(!up, "the enable armed a tunnel after the disable");
    let Err(GuiError::Conflict(message)) = enabled else {
        panic!("a disable after the reservation cancels the enable: {enabled:?}");
    };
    assert_eq!(message, CANCELLED_BY_DISABLE);
    disabled.expect("the disable found the reservation and cancelled it");
}

/// The first test's `disable`, against a store that counts writes of the
/// switch on and parks none: the switch is never written on, even for a
/// moment. The look after the write alone would write it on and then off, and
/// a daemon restarting in between would read it on and put the tunnel back.
#[tokio::test]
async fn a_disable_before_the_reservation_never_writes_the_switch_on() {
    let (core, gate, ops, _arming) = ops_with_a_gated_switch(false).await;
    let held = Arc::clone(&ops.live).lock_owned().await;
    let mut enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    });
    // Nothing can arm while the lock is held, so this is safe to assert now.
    assert!(
        tokio::time::timeout(ANSWERED, &mut enabling).await.is_err(),
        "the enable answered while the serve slot was held"
    );

    let disabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.disable().await }
    });
    let wrote = tokio::time::timeout(Duration::from_secs(10), async {
        while switch(&core).await != Some(false) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    drop(held);
    let enabled = enabling.await.expect("the enable task");
    let _ = disabling.await;
    let up = ops.status().await.enabled;
    // In case the enable armed after all: take it down before judging.
    let _ = ops.disable().await;

    assert!(wrote.is_ok(), "the disable never wrote the switch");
    assert!(!up, "the enable armed a tunnel after the disable");
    assert_eq!(
        gate.on_writes.load(Ordering::SeqCst),
        0,
        "the enable wrote the switch on before taking it back"
    );
    let Err(GuiError::Conflict(message)) = enabled else {
        panic!("a disable before the reservation cancels the enable: {enabled:?}");
    };
    assert_eq!(message, CANCELLED_BY_DISABLE);
}

#[cfg(test)]
#[path = "enable_race_write_tests.rs"]
mod enable_race_write_tests;
