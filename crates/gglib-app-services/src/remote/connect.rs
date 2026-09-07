//! The connect side of ADR 0012: this machine reaching another.
//!
//! `RemoteOps` owns both halves of the tunnel, and this file is the half
//! where this daemon is the laptop. The local listener it binds does **not**
//! inject `Authorization` (ADR 0012, decision 7): gglib's own commands attach
//! the stored key, and a third-party client pointed at the port supplies it
//! as its API key.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gglib_core::events::AppEvent;
use modelpipe::{ConnectError, ConnectHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::pairing_string::{self, Parsed};
use super::slot::{Busy, Taken};
use super::stored_pairing::names_the_same_machine;
use super::types::{ConnectRequest, ConnectSnapshot, Connected};
use super::{RemoteOps, redeem};
use crate::error::GuiError;

#[path = "connect_dial.rs"]
mod connect_dial;

/// How long a teardown lets in-flight requests finish before cutting them.
pub(super) const DRAIN: Duration = Duration::from_secs(5);

/// One live connect side and the task watching it.
pub(super) struct LiveConnect {
    handle: Arc<ConnectHandle>,
    ticket_fingerprint: String,
    /// Which `connect` this is, so a watcher that outlives its connection
    /// cannot take down the next one.
    generation: u64,
    watcher: CancellationToken,
}

impl LiveConnect {
    /// Which `connect` built this, for a watcher checking it is still the
    /// connection it was given.
    pub(super) const fn generation(&self) -> u64 {
        self.generation
    }
}

impl RemoteOps {
    /// Reach another machine: bind a loopback port here that is its proxy.
    ///
    /// With a `<ticket>-<code>` pairing, redeems the code through the tunnel
    /// for the far machine's API key and stores the two as one
    /// [`RemotePairing`](gglib_core::RemotePairing), so later sessions need
    /// only the ticket — or nothing, since the ticket is part of the record.
    ///
    /// Without a code, the dial is admitted only when the stored pairing
    /// names *that* machine. "Some key is stored" was the old test, and it
    /// admitted a bare ticket for machine B on the strength of machine A's
    /// key: connected, `status` reporting a pairing, and every request 401.
    ///
    /// The connect side is *reserved* rather than held for the length of
    /// the dial, so `status` and `disconnect` answer throughout.
    ///
    /// # Errors
    ///
    /// `Conflict` when already connected, when another dial is in flight,
    /// or when `disconnect` took the slot while this one was dialling;
    /// `ValidationFailed` for a pairing string that does not parse, a bare
    /// ticket for a machine this one holds no key for, or a code the far
    /// side refuses; `Unavailable` when the peer cannot be reached;
    /// `Internal` when settings cannot be written.
    pub async fn connect(&self, request: ConnectRequest) -> Result<Connected, GuiError> {
        // Refused before any work is done, so `connect` while connected
        // still says "already connected" rather than reporting the first
        // thing it happens to find wrong with the arguments. The lock is
        // gone by the end of this line; the reservation below is the one
        // that actually holds the slot.
        if let Some(busy) = self.live_connect.lock().await.busy() {
            return Err(busy_dialling(&busy));
        }
        let settings = self.settings().await?;
        let Parsed { ticket, code } = match request.pairing.as_deref() {
            Some(pairing) => pairing_string::parse(pairing).map_err(GuiError::ValidationFailed)?,
            None => {
                let stored = settings.remote_pairing.as_ref().ok_or_else(|| {
                    GuiError::ValidationFailed(
                        "this machine has not connected to a remote before — give it the pairing \
                         string `gglib remote enable` showed there"
                            .to_owned(),
                    )
                })?;
                pairing_string::parse(&stored.ticket).map_err(GuiError::ValidationFailed)?
            }
        };
        // The key this machine holds *for the machine about to be dialled*,
        // which is the only key that means anything: one is issued by the
        // machine whose code was redeemed, so presenting it to any other is
        // a 401 dressed up as a working pairing.
        let held = settings
            .remote_pairing
            .as_ref()
            .filter(|stored| names_the_same_machine(stored, &ticket));
        if code.is_none() && held.is_none() {
            return Err(GuiError::ValidationFailed(
                "this machine holds no key for that remote — pair once with the full \
                 `<ticket>-<code>` string from `gglib remote enable`"
                    .to_owned(),
            ));
        }

        // Reserve the slot, then let the lock go for the dial. Holding it
        // across `modelpipe::connect` is what made `gglib remote status`
        // blow the five seconds the CLI gives it, and `gglib remote
        // disconnect` — the one command that ends a hanging connect — queue
        // behind the connect it was cancelling.
        let generation = self.connect_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = self
            .live_connect
            .lock()
            .await
            .reserve(generation)
            .map_err(|busy| busy_dialling(&busy))?;

        let dialled = self
            .dial(&ticket, code, held.cloned(), &request, generation, &cancel)
            .await;
        if dialled.is_err() {
            self.live_connect.lock().await.release(generation);
        }
        dialled
    }

    /// Close the loopback port; the far machine is unaffected and the stored
    /// pairing stays.
    ///
    /// Also ends a `connect` that is still dialling, which is the case it
    /// exists for and could not reach: it waited on the same mutex the dial
    /// was holding, so the command for cancelling a hanging connect hung
    /// behind it.
    ///
    /// # Errors
    ///
    /// `Conflict` when nothing is connected and nothing is dialling.
    pub async fn disconnect(&self) -> Result<(), GuiError> {
        match self.live_connect.lock().await.take() {
            Taken::Value(LiveConnect {
                handle, watcher, ..
            }) => {
                watcher.cancel();
                if !handle.shutdown_timeout(DRAIN).await {
                    warn!("remote connection drain hit its deadline; remaining requests were cut");
                }
                info!("disconnected from the remote");
                self.emitter.emit(AppEvent::remote_disconnected());
                Ok(())
            }
            // Nothing is bound yet, so there is nothing to drain — and
            // nothing was ever announced as connected, so nothing is
            // announced as gone. The dial finds the slot taken and shuts
            // down whatever it managed to build.
            Taken::Cancelled => {
                info!("cancelled a remote connect that was still dialling");
                Ok(())
            }
            Taken::Empty => Err(GuiError::Conflict("not connected to a remote".to_owned())),
        }
    }

    /// Stop the far daemon, then disconnect. A one-way door: nothing brings
    /// it back except someone at that machine (ADR 0012, decision 7).
    ///
    /// # Errors
    ///
    /// `Conflict` when not connected; `ValidationFailed` when no key is
    /// stored or the far side refuses it; `Unavailable` when the request did
    /// not get through.
    pub async fn kill_remote(&self) -> Result<(), GuiError> {
        let base_url = {
            let live = self.live_connect.lock().await;
            // A dial in flight is not a remote that can be stopped: there
            // is no port to send the shutdown through yet.
            let Some(live) = live.full() else {
                return Err(GuiError::Conflict(
                    "not connected to a remote — `gglib remote connect` first".to_owned(),
                ));
            };
            live.handle.base_url()
        };
        let key = self
            .settings()
            .await?
            .remote_pairing
            .map(|stored| stored.api_key)
            .ok_or_else(|| {
                GuiError::ValidationFailed(
                    "this machine holds no key for the remote, so it cannot stop it".to_owned(),
                )
            })?;
        redeem::kill(&base_url, &key).await?;
        // The far side is going away; take this side down before its
        // watcher reports the closed pipe as a surprise.
        self.disconnect().await
    }

    /// The connect side for the status surface.
    pub(super) async fn connect_snapshot(&self) -> Option<ConnectSnapshot> {
        let live = self.live_connect.lock().await;
        live.full().map(|live| ConnectSnapshot {
            port: live.handle.local_addr().port(),
            base_url: live.handle.base_url(),
            ticket_fingerprint: live.ticket_fingerprint.clone(),
            path: live.handle.status().as_str().to_owned(),
        })
    }

    async fn settings(&self) -> Result<gglib_core::Settings, GuiError> {
        self.core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("could not read settings: {e}")))
    }
}

/// A connect side that is already taken, as the person who typed the
/// command needs to hear it.
fn busy_dialling(busy: &Busy) -> GuiError {
    GuiError::Conflict(
        match busy {
            Busy::Filling => "already dialling a remote — wait for it, or `gglib remote disconnect` to give up on it",
            Busy::Full => "already connected to a remote — `gglib remote disconnect` first",
        }
        .to_owned(),
    )
}

/// A dial that `disconnect` ended before it could be installed.
///
/// A conflict rather than a failure: nothing went wrong with the dial, it
/// was simply no longer wanted by the time it finished.
pub(super) fn cancelled() -> GuiError {
    GuiError::Conflict("the connect was cancelled by `gglib remote disconnect`".to_owned())
}

/// A `ConnectError` as the person who typed `connect` needs to hear it.
fn connect_error(e: ConnectError, port: Option<u16>) -> GuiError {
    match e {
        // One producer left at modelpipe 0.3.0, and it is not a machine
        // that is off: `transport::addr_from`, for a ticket whose endpoint
        // id is not a curve point. A peer that is merely *absent* is no
        // longer reported here at all — `connect` now returns as soon as
        // the local port is bound, and waiting for the machine is
        // `first_contact`'s, which is where that sentence went. iroh defers
        // the curve check further still, so nothing produces this today;
        // the arm stays because that is iroh's choice to revisit, not this
        // repo's, and `ConnectError` is `#[non_exhaustive]`.
        //
        // Deliberately NOT in `docs/remote.md`'s troubleshooting table. A
        // sentence nobody can be shown is noise there, and the cause a reader
        // would reach for — a ticket copied wrong — produces
        // `pairing_string::parse`'s error instead, one guard earlier.
        ConnectError::PeerUnreachable => GuiError::ValidationFailed(
            "that pairing string names an address nobody could be at — copy it again from \
             `gglib remote enable` on the far machine"
                .to_owned(),
        ),
        ConnectError::Bind(err) => GuiError::Conflict(format!(
            "could not bind 127.0.0.1:{}: {err}",
            port.map_or_else(|| "<free port>".to_owned(), |p| p.to_string())
        )),
        ConnectError::InvalidRelay { url } => {
            GuiError::ValidationFailed(format!("`{url}` is not a relay URL"))
        }
        other => GuiError::Internal(format!("could not connect: {other}")),
    }
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod connect_tests;

#[cfg(test)]
#[path = "connect_race_tests.rs"]
mod connect_race_tests;

#[cfg(test)]
#[path = "connect_gate_tests.rs"]
mod connect_gate_tests;
