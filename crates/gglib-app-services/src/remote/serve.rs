//! The serve side of ADR 0012: this machine answering another.
//!
//! Carved off `mod.rs`, which is at its size budget, and along the seam the
//! connect side already has: `connect.rs` is the laptop, this is the
//! desktop, and `mod.rs` keeps the type, the slots and the status surface
//! that reads both.
//!
//! Arming is slow — up to a five-second settings-cache window and then ten
//! seconds waiting for a relay — and `enable` used to hold the serve slot's
//! mutex for all of it, which is a quarter of a minute in which
//! `gglib remote status` could not answer. It gives the CLI five seconds.

use std::sync::atomic::Ordering;

use super::RemoteOps;
use super::pairing::Offer;
use super::serve_switch::busy_serving;
use super::types::{EnableRequest, Enabled};
use crate::error::GuiError;

impl RemoteOps {
    /// Bring the tunnel up in front of the running proxy and arm a pairing.
    ///
    /// Starts the proxy if it is not running; settles the key the tunnel
    /// enforces (minting and persisting one when nothing enforces anything
    /// yet — which puts a bearer requirement on the local proxy too, see
    /// ADR 0012); binds the stored identity; grants the pairing code at the
    /// edge, bounded so wrong bearers burn it; and watches for a rotation.
    /// The slot is reserved rather than held, so everything slow happens with
    /// the lock released and `status` answers throughout.
    ///
    /// # Errors
    ///
    /// `Conflict` when already enabled, or when `disable` took the slot
    /// while this was arming; whatever starting the proxy returns;
    /// `Internal` when settings cannot be written or the tunnel cannot bind.
    pub async fn enable(&self, request: EnableRequest) -> Result<Enabled, GuiError> {
        // Already serving, and asked to invite: a code on the live session
        // rather than "already enabled". The alternative is telling someone
        // to `disable` first, which drops every device already using the
        // tunnel in order to add one. `invite_if_up` has the rest.
        if request.invite
            && let Some(enabled) = self.invite_if_up().await?
        {
            return Ok(enabled);
        }
        // Read here rather than inside `arm`, so that `resume_arm` — which
        // builds its request from stored flags — cannot offer a code however
        // those flags are written. The switch is not where an invite lives.
        let offer = if request.invite {
            Offer::Code
        } else {
            Offer::Silent
        };
        self.turn_on(request, offer).await
    }

    /// Bringing the tunnel up, with `offer` the one thing `enable` and
    /// `resume_arm` differ in — so the rest cannot drift apart.
    pub(super) async fn turn_on(
        &self,
        request: EnableRequest,
        offer: Offer,
    ) -> Result<Enabled, GuiError> {
        if let Some(busy) = self.live.lock().await.busy() {
            return Err(busy_serving(&busy));
        }
        // Subscribed before the address is read, because the channel
        // publishes exits and never starts: a subscription made afterwards
        // would miss a proxy that fell over in between, and this tunnel
        // would front a dead port for the rest of the session.
        let proxy_exit = self.proxy.exit_receiver();
        // Before the reservation on purpose: starting the proxy is the
        // caller's own slow step and has its own guard, and holding the
        // serve slot across it would refuse a second `enable` with the
        // wrong sentence.
        let addr = self.proxy.ensure_running().await?;

        let generation = self.enable_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = self
            .live
            .lock()
            .await
            .reserve(generation)
            .map_err(|busy| busy_serving(&busy))?;

        // The switch and the flags are written before the tunnel binds, not
        // after: `arm` can take fifteen seconds and a daemon killed inside
        // that window should come back up serving, because a person has
        // already said they want this machine reachable. A switch on with
        // nothing bound is recoverable — `resume` arms it. The reverse, a
        // bound tunnel nobody recorded, comes back down at the next restart
        // for no reason a person could see.
        //
        // A write that fails gives the slot back, or it would read as an arm
        // on its way until someone ran `disable`.
        if let Err(e) = self.remember_enabled(&request).await {
            self.live.lock().await.release(generation);
            return Err(e);
        }

        let armed = self
            .arm(request, &addr, generation, &cancel, proxy_exit, offer)
            .await;
        if armed.is_err() {
            self.live.lock().await.release(generation);
        }
        armed
    }
}
