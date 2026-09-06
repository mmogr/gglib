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
/// **The drain comes before the pairing is cleared, and that ordering is the
/// point.** A laptop's `POST /remote/pair` can cross the tunnel edge — and
/// spend the one-time grant that is the only reason it got in — a
/// millisecond before someone types `gglib remote disable` here. Clearing
/// the pairing first means that request arrives at a gateway with nothing
/// armed and is answered with the same flat `401` a wrong code gets, which
/// the laptop renders as "it may have expired, been used already, or been
/// burned by wrong attempts". None of those is true, the code is spent
/// either way, and the operator is sent to re-run `enable` on a machine that
/// was working. Draining first lets that request buy the key it was minted
/// for.
///
/// It opens no window on the tunnel in exchange: `shutdown_timeout` closes
/// admission before it waits, so what the drain protects is exactly the
/// requests that were already inside — no code can be redeemed against this
/// tunnel after this call begins.
///
/// It does open one on the *gateway*, which is why the reset names a
/// session. Both callers release the `live` lock before calling this (the
/// alternative is a `status` that blocks for the whole drain), so a fresh
/// `enable` can arm a new session while this one is still draining. Clearing
/// whatever is armed would then wipe that new session — the same false
/// "expired, used already, or burned" the ordering above exists to prevent,
/// only aimed at a tunnel that is up. So the epoch this tunnel was armed
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
    use std::sync::{Arc, Mutex};

    use gglib_core::ports::{NoopEmitter, PairingOutcome, RemoteGatewayPort as _};
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::remote::pairing::PAIRING_TTL;

    const CODE: &str = "483920";
    const KEY: &str = "the-desktop-key";

    /// A gateway with one session armed, and the epoch that session was
    /// armed under — which is what the `Live` a teardown is given carries.
    fn gateway() -> (Arc<RemoteGateway>, u64) {
        let gateway = Arc::new(RemoteGateway::new(Arc::new(NoopEmitter)));
        let epoch = gateway.begin_session(CODE.to_owned(), KEY.to_owned(), PAIRING_TTL, true);
        (gateway, epoch)
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

    /// A handle that redeems the pairing code while it drains — the laptop's
    /// request that crossed the edge just before the shutdown, arriving at
    /// the gateway from inside the drain, which is where it arrives in
    /// production too.
    struct RedeemingDrain {
        gateway: Arc<RemoteGateway>,
        outcome: Mutex<Option<PairingOutcome>>,
    }

    impl Drain for RedeemingDrain {
        fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
            let outcome = self.gateway.redeem_pairing_code(CODE, Some("3ca82708b995"));
            *self.outcome.lock().unwrap() = Some(outcome);
            std::future::ready(true)
        }
    }

    /// The one-time grant is spent by crossing the edge, so this request has
    /// already paid for the key. Answering it with the refusal a wrong code
    /// gets tells the operator three things that are all false and sends
    /// them back to a machine that was working.
    #[tokio::test]
    async fn a_redeem_still_in_flight_when_the_tunnel_goes_down_gets_the_key_it_paid_for() {
        let (gateway, epoch) = gateway();
        let handle = Arc::new(RedeemingDrain {
            gateway: Arc::clone(&gateway),
            outcome: Mutex::new(None),
        });
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert_eq!(
            *handle.outcome.lock().unwrap(),
            Some(PairingOutcome::Granted(KEY.to_owned())),
            "the pairing has to still be armed while the drain runs"
        );
    }

    /// And no longer than that. The code was shown for one session; once the
    /// drain is over nothing is left inside to redeem it, and a code that
    /// outlived its tunnel would be a credential with no listener behind it.
    #[tokio::test]
    async fn the_pairing_does_not_outlive_the_session_it_was_armed_for() {
        let (gateway, epoch) = gateway();
        let handle = Arc::new(Drained);
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(!gateway.pairing.active());
        assert_eq!(
            gateway.redeem_pairing_code(CODE, None),
            PairingOutcome::Rejected,
            "the session is over, and so is its code"
        );
        assert!(!gateway.mcp_allowed(), "and so is its /mcp grant");
    }

    /// The rotation poll and the proxy watcher both hold this token. Neither
    /// may still be following a tunnel that has gone.
    #[tokio::test]
    async fn taking_a_tunnel_down_stops_what_was_following_it() {
        let (gateway, epoch) = gateway();
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

        let (gateway, epoch) = gateway();
        let handle = Arc::new(NeverDrains);
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert!(!gateway.pairing.active());
    }

    /// What the `enable` that lands inside the drain arms.
    const NEXT_CODE: &str = "111111";
    const NEXT_KEY: &str = "the-next-desktop-key";

    /// A handle that arms a fresh session while it drains — the `enable`
    /// that arrives in the five seconds this teardown spends draining.
    /// Neither caller of `take_down` holds the `live` lock across it, so
    /// that `enable` finds the slot empty, succeeds, and hands somebody a
    /// pairing string.
    struct ArmingDrain {
        gateway: Arc<RemoteGateway>,
    }

    impl Drain for ArmingDrain {
        fn drain(&self, _grace: Duration) -> impl Future<Output = bool> + Send {
            self.gateway.begin_session(
                NEXT_CODE.to_owned(),
                NEXT_KEY.to_owned(),
                PAIRING_TTL,
                true,
            );
            std::future::ready(true)
        }
    }

    /// The session that replaced this one is not this one's to end. A
    /// teardown that cleared whatever it found would burn a code the
    /// operator is holding — answered with the same "expired, used already,
    /// or burned by wrong attempts" that the drain-before-clear ordering
    /// exists to eliminate — and revoke an `/mcp` grant that was just asked
    /// for.
    #[tokio::test]
    async fn a_session_armed_while_the_teardown_drained_survives_it() {
        let (gateway, epoch) = gateway();
        let handle = Arc::new(ArmingDrain {
            gateway: Arc::clone(&gateway),
        });
        let (live, _cancel) = live(&handle, epoch);

        take_down(live, &gateway).await;

        assert_eq!(
            gateway.redeem_pairing_code(NEXT_CODE, None),
            PairingOutcome::Granted(NEXT_KEY.to_owned()),
            "the newer session's code has to still redeem"
        );
        assert!(
            gateway.mcp_allowed(),
            "and its /mcp grant has to still be granted"
        );
    }
}
