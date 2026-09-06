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

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use gglib_core::events::AppEvent;
use gglib_core::services::SETTINGS_CACHE_TTL;
use gglib_core::{SettingsUpdate, access};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::key::{self, KeyDecision};
use super::pairing::PAIRING_TTL;
use super::rotation::rotation_poll;
use super::slot::{Busy, Taken};
use super::types::{EnableRequest, Enabled};
use super::{DRAIN, Live, RemoteOps, WAIT_ONLINE};
use crate::error::GuiError;

impl RemoteOps {
    /// Bring the tunnel up in front of the running proxy and arm a pairing.
    ///
    /// Starts the proxy if it is not running; settles the key the tunnel
    /// enforces (minting and persisting one when nothing enforces anything
    /// yet — which puts a bearer requirement on the local proxy too, see
    /// ADR 0012); binds a fresh identity; grants the pairing code once at the
    /// tunnel edge; and starts watching settings for a rotation.
    ///
    /// The serve slot is reserved rather than held: everything slow happens
    /// with the lock released, so `status` answers throughout and `disable`
    /// can give up on an arming that is taking too long.
    ///
    /// # Errors
    ///
    /// `Conflict` when already enabled, or when `disable` took the slot
    /// while this was arming; whatever starting the proxy returns;
    /// `Internal` when settings cannot be written or the tunnel cannot bind.
    pub async fn enable(&self, request: EnableRequest) -> Result<Enabled, GuiError> {
        if let Some(busy) = self.live.lock().await.busy() {
            return Err(busy_serving(&busy));
        }
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

        let armed = self.arm(request, &addr, generation, &cancel).await;
        if armed.is_err() {
            self.live.lock().await.release(generation);
        }
        armed
    }

    /// Everything from the key to the armed pairing, with the slot already
    /// reserved.
    ///
    /// Split out for the reason `dial` is: it is the span in which the
    /// reservation is held while the lock is not, and one function with one
    /// caller means one place that gives the slot back.
    async fn arm(
        &self,
        request: EnableRequest,
        addr: &SocketAddr,
        generation: u64,
        cancel: &CancellationToken,
    ) -> Result<Enabled, GuiError> {
        let (key, pinned) = self.settle_key(cancel).await?;

        let mut opts = modelpipe::ServeOptions::default();
        opts.auth = modelpipe::TokenPolicy::Supplied(key.clone());
        opts.relay = request.relay;
        // A fresh identity every time: the ticket dies with the session and
        // revocation is the restart (ADR 0012, decision 4).
        opts.identity = None;
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        opts.wait_online = Some(WAIT_ONLINE);
        let handle = modelpipe::serve(&format!("http://{addr}"), opts)
            .await
            .map_err(|e| GuiError::Internal(format!("could not start the remote tunnel: {e}")))?;
        let handle = Arc::new(handle);

        let code = access::generate_pairing_code();
        handle
            .grant_once(code.clone(), PAIRING_TTL)
            .map_err(|e| GuiError::Internal(format!("could not arm the pairing code: {e}")))?;

        // The slot is claimed *before* the gateway is armed. A `disable`
        // that landed during the bind has already reset the session, and
        // arming a pairing after it would leave a live code for a tunnel
        // this call is about to take down.
        let rotation = CancellationToken::new();
        let live = Live {
            handle: Arc::clone(&handle),
            rotation: rotation.clone(),
        };
        if !self.live.lock().await.install(generation, live) {
            if !handle.shutdown_timeout(DRAIN).await {
                warn!("remote tunnel drain hit its deadline; remaining requests were cut");
            }
            return Err(GuiError::Conflict(
                "the enable was cancelled by `gglib remote disable`".to_owned(),
            ));
        }
        self.gateway
            .pairing
            .begin(code.clone(), key.clone(), PAIRING_TTL);
        self.gateway.set_mcp_allowed(request.allow_mcp);
        if !pinned {
            tokio::spawn(rotation_poll(
                Arc::clone(&self.core),
                Arc::clone(&handle),
                Arc::clone(&self.gateway),
                key,
                rotation,
            ));
        }

        let ticket = handle.ticket();
        let fingerprint = ticket.fingerprint();
        info!(ticket = %fingerprint, mcp = request.allow_mcp, "remote tunnel enabled");
        self.emitter.emit(AppEvent::remote_enabled(fingerprint));

        let ticket = ticket.to_string();
        Ok(Enabled {
            pairing: format!("{ticket}-{code}"),
            ticket,
            code,
            expires_in_s: PAIRING_TTL.as_secs(),
        })
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
        match self.live.lock().await.take() {
            Taken::Value(Live { handle, rotation }) => {
                rotation.cancel();
                self.gateway.reset_session();
                if !handle.shutdown_timeout(DRAIN).await {
                    warn!("remote tunnel drain hit its deadline; remaining requests were cut");
                }
                info!("remote tunnel disabled");
                self.emitter.emit(AppEvent::remote_disabled());
                Ok(())
            }
            // Nothing is bound and no pairing is armed yet — `arm` claims
            // the slot before it touches the gateway — so there is nothing
            // to reset and nothing to announce. The arming call finds the
            // slot gone and takes down whatever it built.
            Taken::Cancelled => {
                info!("cancelled a remote enable that was still arming");
                Ok(())
            }
            Taken::Empty => Err(GuiError::Conflict(
                "remote access is not enabled".to_owned(),
            )),
        }
    }

    /// The key the tunnel enforces, minting and persisting one first when
    /// nothing enforces anything yet. Returns whether it is pinned.
    async fn settle_key(&self, cancel: &CancellationToken) -> Result<(String, bool), GuiError> {
        let settings = self
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("could not read settings: {e}")))?;
        match key::decide(
            self.proxy.effective_api_key(),
            settings.proxy_api_key.as_deref(),
        ) {
            KeyDecision::Use { key, pinned } => Ok((key, pinned)),
            KeyDecision::Mint(key) => {
                self.core
                    .settings()
                    .update(SettingsUpdate {
                        proxy_api_key: Some(Some(key.clone())),
                        ..SettingsUpdate::default()
                    })
                    .await
                    .map_err(|e| GuiError::Internal(format!("could not store the API key: {e}")))?;
                // The proxy's tracking policy reads settings through a cache;
                // handing out a ticket before the local door is locked would
                // open a window the whole design exists to close. Cut short
                // by a `disable`, which is not a shortcut: the key is
                // already written and the caller is about to give up.
                info!("minted an API key for the proxy; waiting for it to take effect");
                tokio::select! {
                    () = cancel.cancelled() => {}
                    () = tokio::time::sleep(SETTINGS_CACHE_TTL) => {}
                }
                Ok((key, false))
            }
        }
    }
}

/// A serve side that is already taken, as the person who typed the command
/// needs to hear it.
fn busy_serving(busy: &Busy) -> GuiError {
    GuiError::Conflict(
        match busy {
            Busy::Filling => {
                "remote access is already being enabled — wait for the ticket, or \
                 `gglib remote disable` to give up on it"
            }
            Busy::Full => {
                "remote access is already enabled — `gglib remote disable` first to mint a new \
                 ticket"
            }
        }
        .to_owned(),
    )
}
