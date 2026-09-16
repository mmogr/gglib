//! Taking a live tunnel down, in an order the far machine can survive.
//!
//! One sequence, two callers: `disable`, and the watcher that follows the
//! proxy this tunnel fronts. Both end a session, and a session ends in three
//! moves whose order is the only thing this file is about.

use std::future::Future;
use std::time::Duration;

use tracing::warn;

use super::gateway::RemoteGateway;
use super::{DRAIN, Live};

/// What taking a tunnel down needs of the handle: stop admitting, let what
/// was already admitted finish, and say whether it did.
///
/// A trait over one method with one production implementation, for the same
/// reason [`Live`] is generic over its handle — a real `modelpipe::ServeHandle`
/// exists nowhere but in front of a bound listener. It buys the one thing a
/// test of this order needs: standing in for what happens *during* a drain.
pub(super) trait Drain {
    /// Stop admitting and drain; `true` when everything in flight finished
    /// inside `grace`, `false` when the deadline came first and the rest
    /// were cut.
    fn drain(&self, grace: Duration) -> impl Future<Output = bool> + Send;
}

impl Drain for modelpipe::ServeHandle {
    fn drain(&self, grace: Duration) -> impl Future<Output = bool> + Send {
        self.shutdown_timeout(grace)
    }
}

/// End a session: stop the tasks the tunnel owns, drain it, and only then
/// forget what the session was holding.
///
/// **The drain comes before the session is reset.** What was admitted before
/// `disable` finishes under the session it was admitted to, `/mcp` grant
/// included. A pairing is the exception: modelpipe withdraws a live invite as
/// the listener closes, which is where the drain starts, so a pairing request
/// whose code the edge has not redeemed by then is refused. A device that
/// redeemed before it is recorded whichever runs first, because whoever takes
/// an invite out of the gateway records how it ended.
///
/// It opens no window on the tunnel in exchange: `shutdown_timeout` closes
/// admission before it waits, and modelpipe withdraws the invite as the
/// listener closes, so the drain gives no code a chance to be redeemed.
///
/// It does open one on the *gateway*, which is why the reset names a
/// session. Both callers release the `live` lock before calling this (the
/// alternative is a `status` that blocks for the whole drain), so a fresh
/// `enable` can arm a new session while this one is still draining. Clearing
/// whatever is armed would then wipe that new session, withdrawing the code
/// the operator was just handed on a tunnel that is up. So the epoch this
/// tunnel was armed
/// under is handed back, and a superseded teardown clears nothing.
pub(super) async fn take_down<H: Drain>(live: Live<H>, gateway: &RemoteGateway) {
    // First, because it is what stops anything following a tunnel that is
    // ending: this token is the rotation poll's and the proxy watcher's.
    live.cancel.cancel();
    if !live.handle.drain(DRAIN).await {
        warn!("remote tunnel drain hit its deadline; remaining requests were cut");
    }
    // Last, and only this session's: a code shown for a session that has
    // ended cannot outlive it, and the `/mcp` grant and the paired flag
    // belong to the same session.
    gateway.reset_session_if(live.epoch);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gglib_core::ports::{NoopEmitter, RemoteGatewayPort as _};
    use tokio::sync::mpsc::unbounded_channel;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::remote::pairing::pairing_tests::FakeInvite;
    use crate::remote::roster::Note;

    const DEVICE: &str = "dev-0a1b2c3d";

    /// A gateway with one session armed and an invite open on it; the epoch
    /// that session was armed under, which is what the `Live` a teardown is
    /// given carries; and the invite.
    fn gateway() -> (Arc<RemoteGateway>, u64, Arc<FakeInvite>) {
        let gateway = Arc::new(RemoteGateway::new(Arc::new(NoopEmitter)));
        let epoch = gateway.begin_session(true);
        let invite = FakeInvite::new();
        gateway.offer_pairing(epoch, DEVICE.to_owned(), Box::new(Arc::clone(&invite)));
        (gateway, epoch, invite)
    }

    fn live<H>(handle: &Arc<H>, epoch: u64) -> (Live<H>, CancellationToken) {
        let cancel = CancellationToken::new();
        (
            Live {
                handle: Arc::clone(handle),
                cancel: cancel.clone(),
                epoch,
            },
            cancel,
        )
    }

    /// A handle with nothing in flight, for the tests that are about what
    /// happens on either side of the drain rather than during it.
    struct Drained;

    impl Drain for Drained {
        fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
            std::future::ready(true)
        }
    }

    /// A handle across whose drain the open invite turns out redeemed: a
    /// device that redeemed just before `disable`, whose outcome nothing has
    /// recorded by the time the drain returns. The redemption is staged inside
    /// the drain because that is the last moment it can be found unrecorded.
    struct RedeemingDrain(Arc<FakeInvite>);

    impl Drain for RedeemingDrain {
        fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
            self.0.redeem(DEVICE, None);
            std::future::ready(true)
        }
    }

    /// The device paid for its key with the code and holds it. Leaving it off
    /// the roster would list it as an invite nobody took, and `forget` on
    /// that row would be a revocation the operator did not know they were
    /// making.
    #[tokio::test]
    async fn a_device_that_redeemed_just_before_the_teardown_is_still_recorded() {
        let (gateway, epoch, invite) = gateway();
        let (sender, mut inbox) = unbounded_channel();
        gateway.take_notes(sender);
        let handle = Arc::new(RedeemingDrain(invite));
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(
            matches!(inbox.recv().await, Some(Note::Joined { device, .. }) if device == DEVICE),
            "a redemption nothing recorded before the teardown has to reach the roster's writer"
        );
    }

    /// And no longer than that. The code was shown for one session; once the
    /// drain is over nothing is left inside to redeem it, and an invite that
    /// outlived its tunnel would read as a live code with no listener behind
    /// it.
    #[tokio::test]
    async fn the_invite_does_not_outlive_the_session_it_was_opened_for() {
        let (gateway, epoch, invite) = gateway();
        let handle = Arc::new(Drained);
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(!gateway.pairing.active());
        assert!(
            invite.was_withdrawn(),
            "the session is over, and so is its code"
        );
        assert!(!gateway.mcp_allowed(), "and so is its /mcp grant");
    }

    /// The rotation poll and the proxy watcher both hold this token. Neither
    /// may still be following a tunnel that has gone.
    #[tokio::test]
    async fn taking_a_tunnel_down_stops_what_was_following_it() {
        let (gateway, epoch, _invite) = gateway();
        let handle = Arc::new(Drained);
        let (live, cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(cancel.is_cancelled());
    }

    /// A drain that ran out of time is reported and does not stop the rest:
    /// the requests were cut, and leaving the session armed afterwards would
    /// be strictly worse than saying so.
    #[tokio::test]
    async fn a_drain_that_missed_its_deadline_still_ends_the_session() {
        struct NeverDrains;
        impl Drain for NeverDrains {
            fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
                std::future::ready(false)
            }
        }

        let (gateway, epoch, _invite) = gateway();
        let handle = Arc::new(NeverDrains);
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(!gateway.pairing.active());
    }

    /// A handle that arms a fresh session while it drains — the `enable`
    /// that arrives in the five seconds this teardown spends draining.
    /// Neither caller of `take_down` holds the `live` lock across it, so
    /// that `enable` finds the slot empty, succeeds, and hands somebody a
    /// pairing string.
    struct ArmingDrain {
        gateway: Arc<RemoteGateway>,
        next: Arc<FakeInvite>,
    }

    impl Drain for ArmingDrain {
        fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
            let epoch = self.gateway.begin_session(true);
            self.gateway.offer_pairing(
                epoch,
                "dev-4e5f6a7b".to_owned(),
                Box::new(Arc::clone(&self.next)),
            );
            std::future::ready(true)
        }
    }

    /// The session that replaced this one is not this one's to end. A
    /// teardown that cleared whatever it found would withdraw a code the
    /// operator is holding and revoke an `/mcp` grant that was just asked
    /// for.
    #[tokio::test]
    async fn a_session_armed_while_the_teardown_drained_survives_it() {
        let (gateway, epoch, _invite) = gateway();
        let next = FakeInvite::new();
        let handle = Arc::new(ArmingDrain {
            gateway: Arc::clone(&gateway),
            next: Arc::clone(&next),
        });
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(
            gateway.pairing.active(),
            "the newer session's code has to still be redeemable"
        );
        assert!(!next.was_withdrawn());
        assert!(
            gateway.mcp_allowed(),
            "and its /mcp grant has to still be granted"
        );
    }
}
