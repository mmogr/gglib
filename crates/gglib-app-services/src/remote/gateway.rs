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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, PairingOutcome, RemoteGatewayPort};

use super::pairing::Pairing;

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
            emitter,
        }
    }

    /// Arm a session — `code` redeems for `key` for `ttl`, and `/mcp` is
    /// open to tunnelled requests or it is not — and say which session that
    /// is. The number comes back for [`Self::reset_session_if`].
    ///
    /// The paired flag is cleared here as well as there, because a teardown
    /// is not guaranteed to run: the session this replaces may still be
    /// draining, and its teardown will decline to touch anything (that is
    /// what the epoch is for). Nobody has paired with a session that is only
    /// now being armed, and `status` would otherwise report the last one's
    /// answer.
    ///
    /// `code` is `None` when the tunnel is being put back by a restart
    /// rather than by a person: any pairing the previous session left is
    /// cleared and none is armed. A code nobody is watching for is a live
    /// grant nobody spends, and the route that redeems it sits outside the
    /// proxy's bearer group.
    pub(super) fn begin_session(
        &self,
        code: Option<String>,
        key: String,
        ttl: Duration,
        allow_mcp: bool,
    ) -> u64 {
        let mut session = self.session();
        *session += 1;
        match code {
            Some(code) => self.pairing.begin(code, key, ttl),
            None => self.pairing.clear(),
        }
        self.mcp_allowed.store(allow_mcp, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
        *session
    }

    /// Reset everything session `epoch` owns — the pairing, the `/mcp`
    /// grant, and the paired flag — unless a later session has taken the
    /// gateway over since. The request counters are history and stay.
    ///
    /// The guard is not defensive: a teardown takes its time. `take_down`
    /// drains for up to `DRAIN` before it gets here, and neither of its
    /// callers holds the `live` lock while it does — holding it would block
    /// `status` for the whole drain. So a `disable` and a fresh `enable` can
    /// overlap, and a teardown that cleared unconditionally would wipe the
    /// session that replaced it: the operator would be handed a pairing
    /// string that answers `Rejected` — "expired, used already, or burned by
    /// wrong attempts", none of it true — and an `/mcp` grant revoked
    /// without a word.
    ///
    /// Read and act under the one lock, which is the reason the epoch is not
    /// a bare atomic: an `enable` landing between a load and the clears
    /// would be wiped by a teardown that had just decided to leave it alone.
    pub(super) fn reset_session_if(&self, epoch: u64) {
        let session = self.session();
        if *session != epoch {
            return;
        }
        self.pairing.clear();
        self.mcp_allowed.store(false, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
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

    pub(super) fn last_peer(&self) -> Option<String> {
        self.last_peer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RemoteGatewayPort for RemoteGateway {
    fn redeem_pairing_code(&self, code: &str, peer: Option<&str>) -> PairingOutcome {
        let outcome = self.pairing.redeem(code);
        if matches!(outcome, PairingOutcome::Granted(_)) {
            self.paired.store(true, Ordering::Relaxed);
            self.emitter
                .emit(AppEvent::remote_paired(peer.map(str::to_owned)));
        }
        outcome
    }

    fn mcp_allowed(&self) -> bool {
        self.mcp_allowed.load(Ordering::Relaxed)
    }

    fn note_tunnelled_request(&self, peer: Option<&str>) {
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
