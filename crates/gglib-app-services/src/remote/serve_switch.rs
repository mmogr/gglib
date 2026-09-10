//! The switch: whether this machine is meant to be reachable, and putting
//! it back that way at boot.
//!
//! A `#[path]` child of `serve.rs`, split off when that file crossed its
//! budget, along the line the file already had. `serve.rs` is about *arming*
//! — binding an endpoint, minting a code, holding the slot — and this is
//! about the standing answer to a different question: does a person want
//! this machine reachable at all? `enable` and `disable` are where the two
//! meet, so they stay there and call in here.
//!
//! The distinction is load-bearing rather than tidy. A daemon stopping is
//! not an answer to that question, which is why `shut_down` exists beside
//! `disable` in the parent; and a resume is the machine acting on an answer
//! already given, with nobody watching.

use gglib_core::RemoteServe;
use gglib_core::SettingsUpdate;
use tracing::{info, warn};

use crate::RemoteOps;
use gglib_core::events::AppEvent;

use crate::error::GuiError;

use super::slot::Taken;

use super::types::EnableRequest;

impl RemoteOps {
    /// Write the switch on and the flags this `enable` was given.
    pub(super) async fn remember_enabled(&self, request: &EnableRequest) -> Result<(), GuiError> {
        self.core
            .settings()
            .update(SettingsUpdate {
                remote_enabled: Some(Some(true)),
                remote_serve: Some(Some(RemoteServe {
                    allow_mcp: request.allow_mcp,
                    relay: request.relay.clone(),
                    discovery: request.discovery,
                })),
                ..SettingsUpdate::default()
            })
            .await
            .map_err(|e| {
                GuiError::Internal(format!("could not record that remote access is on: {e}"))
            })?;
        Ok(())
    }

    /// Bring the tunnel back up the way it was left, at daemon start.
    ///
    /// Reads the switch rather than taking a request, because nobody is
    /// typing: this is the machine doing again what it was told to do once.
    /// Serving needs [`Settings::remote_serve`] — the flags `enable` was
    /// given — and its absence means the switch was never set by an
    /// `enable`, so there is nothing to reproduce and this does nothing.
    ///
    /// Never fails the daemon. A tunnel that cannot be armed at boot is a
    /// machine you reach later rather than a machine that will not start,
    /// and the reason is logged where the other startup steps log theirs.
    ///
    /// # Errors
    ///
    /// None: every failure is logged and swallowed. The `Result` is kept off
    /// the signature deliberately so no caller is tempted to `?` on it.
    pub async fn resume(&self) {
        let settings = match self.core.settings().get().await {
            Ok(settings) => settings,
            Err(e) => {
                warn!("could not read settings to resume remote access: {e}");
                return;
            }
        };
        if settings.remote_enabled != Some(true) {
            return;
        }
        let Some(serve) = settings.remote_serve.clone() else {
            warn!("remote access is switched on but no serve options were stored; not resuming");
            return;
        };
        let request = EnableRequest {
            allow_mcp: serve.allow_mcp,
            relay: serve.relay,
            discovery: serve.discovery,
        };
        // No pairing code is minted here. `enable` shows a code because a
        // person is watching for it; a resume has no audience, and a code
        // nobody sees is a live grant nobody spends. Devices already paired
        // hold a key and need no code; a new one runs `enable` again.
        match self.enable(request).await {
            Ok(_) => info!("remote access resumed from settings"),
            Err(e) => warn!("could not resume remote access: {e}"),
        }
    }

    /// Take the tunnel down. The ticket is dead from this moment; the key
    /// stays in settings, because the local proxy has demanded it since
    /// `enable` ran and withdrawing it would break whatever adopted it
    /// (ADR 0012, decision 2).
    ///
    /// Also gives up on an `enable` that is still arming, which nothing
    /// could do while that call held the mutex for its whole fifteen
    /// seconds.
    ///
    /// # Errors
    ///
    /// `Conflict` when nothing is enabled and nothing is arming.
    pub async fn disable(&self) -> Result<(), GuiError> {
        // Off is off across restarts, so the switch is cleared before the
        // slot is read: `Conflict` below means nothing was bound *here*, and
        // a daemon that would otherwise have resumed at the next boot is
        // exactly the case where clearing it matters most. The stored
        // pairings are left alone — this takes the tunnel down, it does not
        // forget anybody.
        if let Err(e) = self
            .core
            .settings()
            .update(SettingsUpdate {
                remote_enabled: Some(Some(false)),
                ..SettingsUpdate::default()
            })
            .await
        {
            warn!("could not record that remote access is off: {e}");
        }
        self.shut_down().await
    }

    /// Take the tunnel down without touching the switch.
    ///
    /// What shutdown calls. A daemon stopping is not a person saying they no
    /// longer want this machine reachable, and the difference is the whole
    /// point of the switch: if the process ending cleared it, remote access
    /// would be off after every reboot and `resume` would never fire — which
    /// is the behaviour the switch exists to end.
    ///
    /// # Errors
    ///
    /// `Conflict` when nothing is enabled and nothing is arming.
    pub async fn shut_down(&self) -> Result<(), GuiError> {
        match self.live.lock().await.take() {
            Taken::Value(live) => {
                super::teardown::take_down(live, &self.gateway).await;
                info!("remote tunnel disabled");
                self.emitter.emit(AppEvent::remote_disabled());
                Ok(())
            }
            // Nothing is bound and no pairing is armed yet — `arm` claims
            // the slot and arms the gateway under one hold of this lock, so
            // a reservation is never a session — and there is therefore
            // nothing to reset and nothing to announce. The arming call
            // finds the slot gone and takes down whatever it built.
            Taken::Cancelled => {
                info!("cancelled a remote enable that was still arming");
                Ok(())
            }
            Taken::Empty => Err(GuiError::Conflict(
                "remote access is not enabled".to_owned(),
            )),
        }
    }
}
