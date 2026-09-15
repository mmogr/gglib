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

use modelpipe::{InviteHandle, InviteOutcome};

use super::gateway::RemoteGateway;
use super::pairing::Invitation;

impl Invitation for InviteHandle {
    fn ended(&self) -> Option<InviteOutcome> {
        Self::ended(self)
    }

    fn withdraw(&self) {
        Self::withdraw(self);
    }
}

/// Wait for the invite behind `handle` to end, then let the gateway record
/// how, if nothing has taken it out of the gateway first.
pub(super) fn watch(gateway: Arc<RemoteGateway>, device: String, handle: InviteHandle) {
    tokio::spawn(async move {
        handle.outcome().await;
        gateway.settle_invite(&device);
    });
}

#[cfg(test)]
#[path = "invite_watch_tests.rs"]
mod invite_watch_tests;
