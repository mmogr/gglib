//! What the proxy is allowed to ask the tunnel's owner.
//!
//! One `Arc<RemoteGateway>` is built with the service graph and handed to
//! `ProxyOps`, which puts it on every proxy it starts. It is therefore
//! always present, whether or not the tunnel is up: with nothing armed it
//! rejects every pairing code, and with the tunnel down `mcp_allowed` is
//! whatever it was last set to, which the proxy never consults because no
//! request is marked as tunnelled.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, PairingOutcome, RemoteGatewayPort};

use tokio::sync::mpsc::UnboundedSender;

use super::pairing::Pairing;
use super::roster::Note;

#[path = "gateway_session.rs"]
mod session;

pub(super) use session::Offered;

/// The tunnel's side of the proxy's questions.
pub struct RemoteGateway {
    pub(super) pairing: Pairing,
    mcp_allowed: AtomicBool,
    paired: AtomicBool,
    /// Which session the three fields above belong to, counted up by
    /// [`RemoteGateway::begin_session`].
    ///
    /// A number behind a lock rather than an atomic, because it is only
    /// worth having if reading it and acting on it are one step — see
    /// [`RemoteGateway::reset_session_if`].
    session: Mutex<u64>,
    tunnelled_requests: AtomicU64,
    /// Unix milliseconds of the last tunnelled request, or a negative
    /// sentinel for "never".
    last_tunnelled_ms: AtomicI64,
    last_peer: Mutex<Option<String>>,
    /// Where roster changes go to be persisted, while a tunnel is up.
    ///
    /// Installed by `arm` and dropped by the teardown, so a note taken with
    /// no tunnel — which cannot happen, since nothing is admitted then — is
    /// discarded rather than queued for a task that will never read it.
    notes: Mutex<Option<UnboundedSender<Note>>>,
    emitter: std::sync::Arc<dyn AppEventEmitter>,
}

impl RemoteGateway {
    pub(crate) fn new(emitter: std::sync::Arc<dyn AppEventEmitter>) -> Self {
        Self {
            pairing: Pairing::default(),
            mcp_allowed: AtomicBool::new(false),
            paired: AtomicBool::new(false),
            session: Mutex::new(0),
            tunnelled_requests: AtomicU64::new(0),
            last_tunnelled_ms: AtomicI64::new(-1),
            last_peer: Mutex::new(None),
            notes: Mutex::new(None),
            emitter,
        }
    }

    // Nothing panics while holding this lock; recovering the guard is the
    // honest answer to an impossible poison.
    fn session(&self) -> std::sync::MutexGuard<'_, u64> {
        self.session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn paired(&self) -> bool {
        self.paired.load(Ordering::Relaxed)
    }

    pub(super) fn tunnelled_requests(&self) -> u64 {
        self.tunnelled_requests.load(Ordering::Relaxed)
    }

    pub(super) fn last_tunnelled_ms(&self) -> Option<i64> {
        let ms = self.last_tunnelled_ms.load(Ordering::Relaxed);
        (ms >= 0).then_some(ms)
    }

    /// Hand roster notes to `sender` until the session ends.
    ///
    /// Replacing the previous sender drops it, which is what ends the
    /// `roster_sync` a superseded session left running — so an `arm` that
    /// raced a teardown cannot leave two writers on one roster.
    pub(super) fn take_notes(&self, sender: UnboundedSender<Note>) {
        *self
            .notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);
    }

    /// Send a note if anybody is listening. Never blocks: the channel is
    /// unbounded, and a closed one means the tunnel went while this request
    /// was in flight, which the roster will learn from the next arm anyway.
    fn note(&self, note: Note) {
        if let Some(sender) = self
            .notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            let _ = sender.send(note);
        }
    }

    pub(super) fn last_peer(&self) -> Option<String> {
        self.last_peer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RemoteGatewayPort for RemoteGateway {
    fn redeem_pairing_code(
        &self,
        code: &str,
        peer: Option<&str>,
        name: Option<&str>,
    ) -> PairingOutcome {
        let outcome = self.pairing.redeem(code);
        if let PairingOutcome::Granted { device, .. } = &outcome {
            self.paired.store(true, Ordering::Relaxed);
            // The roster row is written by `roster_sync`, not here: this runs
            // on the request path and the port is synchronous by contract, so
            // a settings write is not available. What the device called
            // itself is a label, and a label learned microseconds before a
            // crash is not worth an `await` on every tunnelled request.
            self.note(Note::Joined {
                device: device.clone(),
                label: name.map(str::to_owned),
                at_ms: super::roster::now_ms(),
            });
            self.emitter
                .emit(AppEvent::remote_paired(peer.map(str::to_owned)));
        }
        outcome
    }

    fn mcp_allowed(&self) -> bool {
        self.mcp_allowed.load(Ordering::Relaxed)
    }

    fn note_tunnelled_request(&self, peer: Option<&str>, device: Option<&str>) {
        self.tunnelled_requests.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(-1);
        self.last_tunnelled_ms.store(now, Ordering::Relaxed);
        if let Some(peer) = peer {
            *self
                .last_peer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer.to_owned());
        }
        // Advisory, exactly as the counter above is. `roster_sync` debounces
        // the write and drops an id the roster does not know, so a local
        // process forging the markers cannot invent a device or move one it
        // has no key for.
        if let Some(device) = device
            && now >= 0
        {
            self.note(Note::Seen {
                device: device.to_owned(),
                at_ms: now,
            });
        }
    }
}

impl fmt::Debug for RemoteGateway {
    /// State, never secrets: the pending code and the key it stands for are
    /// both credentials, and `ProxyConfig` derives `Debug` over this.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteGateway")
            .field("pairing_active", &self.pairing.active())
            .field("mcp_allowed", &self.mcp_allowed())
            .field("paired", &self.paired())
            .field("tunnelled_requests", &self.tunnelled_requests())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod gateway_tests;
