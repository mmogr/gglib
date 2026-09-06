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
use gglib_core::access;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_runtime::proxy::ProxyStatus;

use super::backend::Backend;
use super::key;
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

        let armed = self
            .arm(request, &addr, generation, &cancel, proxy_exit)
            .await;
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
        proxy_exit: watch::Receiver<ProxyStatus>,
    ) -> Result<Enabled, GuiError> {
        let settled = key::settle(&self.proxy, &self.core).await?;

        let backend = Backend::at(*addr);

        let mut opts = modelpipe::ServeOptions::default();
        opts.auth = modelpipe::TokenPolicy::Supplied(settled.key.clone());
        opts.relay = request.relay;
        // A fresh identity every time: the ticket dies with the session and
        // revocation is the restart (ADR 0012, decision 4).
        opts.identity = None;
        opts.port_mapping = false;
        opts.discovery = request.discovery;
        opts.wait_online = Some(WAIT_ONLINE);
        opts.allow_private_backend = backend.allow_private;
        let handle = modelpipe::serve(&backend.url, opts)
            .await
            .map_err(|e| GuiError::Internal(format!("could not start the remote tunnel: {e}")))?;
        let handle = Arc::new(handle);

        // The last moment anything notices a proxy that went away while the
        // tunnel was binding: the watcher that takes over from here is not
        // spawned until the install below.
        super::backend::refuse_if_gone(&self.proxy, &backend, &handle).await?;

        // The first and only thing this leaves on the machine, and the last
        // point at which leaving nothing is still free: everything that can
        // fail is above it, and the tunnel above it is undone by dropping the
        // handle — nothing can be in flight behind a ticket that has not left
        // this function.
        settled.commit(&self.core, cancel).await?;

        let code = access::generate_pairing_code();
        handle
            .grant_once(code.clone(), PAIRING_TTL)
            .map_err(|e| GuiError::Internal(format!("could not arm the pairing code: {e}")))?;

        // The slot is claimed and the gateway armed under **one** hold of
        // the lock. Claiming first is not enough: dropping the guard wakes
        // whatever `disable` is queued behind it, and on a multi-thread
        // runtime that `disable` runs in parallel with the lines after the
        // guard — resetting the session and taking the tunnel down while
        // this call is still on its way to `begin`. A pairing code armed
        // after that reset stays live for `PAIRING_TTL` on a session that
        // is gone, and `POST /v1/remote/pair` is outside the proxy's bearer
        // group, so anything that can reach the proxy could spend it.
        // Under one guard there are only two things a `disable` can find:
        // a reservation with nothing armed, or a tunnel with its code.
        //
        // Nothing in here is slow — an install, a `Mutex<Option<_>>` and an
        // atomic — which is the whole reason it may share the guard at all.
        let watchers = CancellationToken::new();
        let live = Live {
            handle: Arc::clone(&handle),
            cancel: watchers.clone(),
        };
        let mut slot = self.live.lock().await;
        if !slot.install(generation, live) {
            // Before the drain, which is the slow part this lock may not be
            // held across.
            drop(slot);
            if !handle.shutdown_timeout(DRAIN).await {
                warn!("remote tunnel drain hit its deadline; remaining requests were cut");
            }
            return Err(GuiError::Conflict(
                "the enable was cancelled by `gglib remote disable`".to_owned(),
            ));
        }
        self.gateway
            .pairing
            .begin(code.clone(), settled.key.clone(), PAIRING_TTL);
        self.gateway.set_mcp_allowed(request.allow_mcp);
        drop(slot);
        // Both watchers start only now, and the ordering is load-bearing.
        // `watch_proxy` takes the slot with `take_if`, which looks at a
        // *full* slot and passes over a reservation — so one spawned while
        // `arm` still held the reservation would find nothing, return, and
        // never look again, leaving a live tunnel in front of a dead proxy
        // with nothing following it. `refuse_if_gone` covers the window up
        // to here; from here the watcher does.
        if !settled.pinned {
            tokio::spawn(rotation_poll(
                Arc::clone(&self.core),
                Arc::clone(&handle),
                Arc::clone(&self.gateway),
                settled.key,
                watchers.clone(),
            ));
        }
        super::backend::follow_proxy(self, &handle, proxy_exit, watchers, backend);

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
