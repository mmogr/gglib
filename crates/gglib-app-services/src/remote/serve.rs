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

use tokio::sync::watch;

use super::RemoteOps;
use super::pairing::Offer;
use super::resume_wait::Waited;
use super::serve_switch::{CANCELLED_BY_DISABLE, busy_serving};
use super::types::{EnableRequest, Enabled};
use crate::error::GuiError;

/// Who is turning the tunnel on, which decides two things `turn_on` does.
pub(super) enum Caller {
    /// A person's `enable`, which writes the switch and its flags first.
    Person,
    /// The daemon's own resume, which writes nothing, and hears about a
    /// `disable` since its first line through this subscription.
    Resume(watch::Receiver<u64>),
}

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
    /// A call that arrives while the daemon's own startup resume is putting
    /// the tunnel back waits for it (`resume_wait.rs`), and is answered by
    /// what that resume brought up: a code on it when `invite` is set, and
    /// the session as it stands when it is not. The flags in such a request
    /// do not apply to the session the resume armed, for `answer_if_up`'s
    /// reason. If the resume brought nothing up this arms its own, and if the
    /// wait runs out it meets the ordinary refusal.
    ///
    /// # Errors
    ///
    /// `Conflict` when already enabled, when another call is still arming,
    /// when `disable` took the slot while this was arming, or when one landed
    /// while this waited;
    /// whatever starting the proxy returns; `Internal` when settings cannot
    /// be written or the tunnel cannot bind.
    pub async fn enable(&self, request: EnableRequest) -> Result<Enabled, GuiError> {
        let waited = self.wait_out_the_resume().await;
        if waited == Waited::Cancelled {
            return Err(GuiError::Conflict(CANCELLED_BY_DISABLE.to_owned()));
        }
        // Read here rather than inside `arm`, so that `resume_arm` — which
        // builds its request from stored flags — cannot offer a code however
        // those flags are written. The switch is not where an invite lives.
        let offer = if request.invite {
            Offer::Code
        } else {
            Offer::Silent
        };
        // Already serving, and asked to invite: a code on the live session
        // rather than "already enabled". The alternative is telling someone
        // to `disable` first, which drops every device already using the
        // tunnel in order to add one. And a plain `enable` that waited for a
        // resume is answered by the session the resume armed: it was not up
        // when the person asked, and it is now, which is what they asked
        // for. `answer_if_up` has the rest.
        if (request.invite || waited == Waited::ForAResume)
            && let Some(enabled) = self.answer_if_up(offer).await?
        {
            return Ok(enabled);
        }
        self.turn_on(request, offer, Caller::Person).await
    }

    /// Bringing the tunnel up, with `offer` and `caller` the two things
    /// `enable` and `resume_arm` differ in — so the rest cannot drift apart.
    pub(super) async fn turn_on(
        &self,
        request: EnableRequest,
        offer: Offer,
        caller: Caller,
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
        let cancel = {
            let mut live = self.live.lock().await;
            let cancel = live
                .reserve(generation)
                .map_err(|busy| busy_serving(&busy))?;
            // A `disable` since the resume began found this slot empty, so it
            // had nothing to cancel, and the resume has to notice it itself.
            // Read under the same hold as the reservation: a `disable` that
            // says so after this finds the reservation and cancels it.
            if let Caller::Resume(disables) = &caller
                && disables.has_changed().unwrap_or(false)
            {
                live.release(generation);
                return Err(GuiError::Conflict(CANCELLED_BY_DISABLE.to_owned()));
            }
            cancel
        };

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
        //
        // A resume writes nothing. It read the switch and the flags a moment
        // ago, and writing them back could only undo a `disable` that landed
        // in between.
        if matches!(caller, Caller::Person)
            && let Err(e) = self.remember_enabled(&request).await
        {
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
