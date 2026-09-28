//! Learning how an invite ended, and handing it to the gateway.
//!
//! modelpipe answers the pairing request at the tunnel edge, so nothing in
//! this process sees a device redeem its code. What it can see is the invite's
//! outcome, which the invite's handle waits on. This task waits, then hands the
//! ended invite to the gateway, which records the device.
//!
//! **There is no cancellation token, on purpose.** The task ends when the
//! invite does, and modelpipe ends every live invite as `Withdrawn` when the
//! listener closes, so it cannot outlive the tunnel, and a token would only be
//! a second way to stop it before it had read how the invite ended. Nor does
//! recording depend on this
//! task winning a race: whoever takes an ended invite out of the gateway
//! records it, and `reset_session_if` does so before it lets the roster's
//! writer go.

use std::sync::Arc;

use modelpipe::{InviteHandle, InviteOutcome, PeerId, ServeHandle};
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::gateway::RemoteGateway;
use super::pairing::Invitation;

/// What pinning a redeemed key needs: the listener that holds it, the roster
/// lock `forget` writes under, and the key itself, in hand before the swap.
pub(super) struct Pin {
    pub(super) serving: Arc<ServeHandle>,
    pub(super) roster: Arc<Mutex<()>>,
    pub(super) key: String,
}

impl Invitation for InviteHandle {
    fn ended(&self) -> Option<InviteOutcome> {
        Self::ended(self)
    }

    fn withdraw(&self) {
        Self::withdraw(self);
    }
}

/// Wait for the invite behind `handle` to end, pin a redeemed key to the
/// endpoint that redeemed it, then let the gateway record how the invite
/// ended, if nothing has taken it out of the gateway first.
pub(super) fn watch(gateway: Arc<RemoteGateway>, device: String, handle: InviteHandle, pin: Pin) {
    tokio::spawn(async move {
        if let InviteOutcome::Redeemed { peer, .. } = handle.outcome().await {
            pin_to(pin, &device, peer).await;
        }
        gateway.settle_invite(&device);
    });
}

/// Hold `device`'s key again, admitting only from `peer`.
///
/// modelpipe holds an invite's key unpinned and has no way to pin a live
/// one, so this removes it and adds it back pinned. Under the roster lock,
/// and only when the remove found the key: a `forget` that got there first
/// has already removed it, and adding it back would admit a device no roster
/// lists. `forget`'s second remove covers one landing between the two calls.
/// A pinned add the edge refuses leaves the device not admitted; it pairs
/// again.
async fn pin_to(pin: Pin, device: &str, peer: PeerId) {
    let Pin {
        serving,
        roster,
        key,
    } = pin;
    let _guard = roster.lock().await;
    if !serving.remove_token(device) {
        info!(device = %device, "a device was retired before its key was pinned");
        return;
    }
    match serving.add_token_pinned(device, key, peer) {
        Ok(()) => info!(
            device = %device,
            peer = %peer.fingerprint(),
            "pinned a device's key to the endpoint that redeemed it"
        ),
        Err(e) => warn!(
            device = %device,
            "the tunnel refused to pin a device's key, so it is not admitted; pair it again: {e}"
        ),
    }
}

#[cfg(test)]
#[path = "invite_watch_tests.rs"]
mod invite_watch_tests;

#[cfg(test)]
#[path = "pin_tests.rs"]
mod pin_tests;

#[cfg(test)]
#[path = "pin_race_tests.rs"]
mod pin_race_tests;
