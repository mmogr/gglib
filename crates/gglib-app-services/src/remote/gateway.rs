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

use super::pairing::Pairing;

/// The tunnel's side of the proxy's questions.
pub struct RemoteGateway {
    pub(super) pairing: Pairing,
    mcp_allowed: AtomicBool,
    paired: AtomicBool,
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
            tunnelled_requests: AtomicU64::new(0),
            last_tunnelled_ms: AtomicI64::new(-1),
            last_peer: Mutex::new(None),
            emitter,
        }
    }

    pub(super) fn set_mcp_allowed(&self, allowed: bool) {
        self.mcp_allowed.store(allowed, Ordering::Relaxed);
    }

    /// Reset everything a session owns: the pairing, the `/mcp` grant, and
    /// the paired flag. The request counters are history and stay.
    pub(super) fn reset_session(&self) {
        self.pairing.clear();
        self.mcp_allowed.store(false, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
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
