//! A stand-in for the tunnel's owner, for the proxy's remote-tunnel tests.
//!
//! Implements [`RemoteGatewayPort`] with a single fixed code that redeems
//! once, burns after three misses, and counts what the proxy tells it — the
//! same contract `gglib-app-services` implements over its real pairing
//! state, reduced to what these tests need to observe.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gglib_core::ports::{PairingOutcome, RemoteGatewayPort};

/// The stub. Fields are read by the tests; the port methods write them.
pub(crate) struct StubGateway {
    code: String,
    key: String,
    mcp_allowed: AtomicBool,
    spent: AtomicBool,
    misses: AtomicUsize,
    /// Every request the proxy reported as tunnelled.
    pub(crate) tunnelled: AtomicUsize,
    /// The peer fingerprint on the most recent tunnelled request.
    pub(crate) last_peer: Mutex<Option<String>>,
    /// The peer fingerprint presented with the redeemed code.
    pub(crate) paired_peer: Mutex<Option<String>>,
}

impl StubGateway {
    pub(crate) fn new(code: &str, key: &str, mcp_allowed: bool) -> Self {
        Self {
            code: code.to_owned(),
            key: key.to_owned(),
            mcp_allowed: AtomicBool::new(mcp_allowed),
            spent: AtomicBool::new(false),
            misses: AtomicUsize::new(0),
            tunnelled: AtomicUsize::new(0),
            last_peer: Mutex::new(None),
            paired_peer: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for StubGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubGateway").finish_non_exhaustive()
    }
}

impl RemoteGatewayPort for StubGateway {
    fn redeem_pairing_code(&self, code: &str, peer: Option<&str>) -> PairingOutcome {
        if self.spent.load(Ordering::SeqCst) {
            return PairingOutcome::Rejected;
        }
        if code == self.code {
            self.spent.store(true, Ordering::SeqCst);
            *self.paired_peer.lock().unwrap() = peer.map(str::to_owned);
            return PairingOutcome::Granted(self.key.clone());
        }
        if self.misses.fetch_add(1, Ordering::SeqCst) + 1 >= 3 {
            self.spent.store(true, Ordering::SeqCst);
        }
        PairingOutcome::Rejected
    }

    fn mcp_allowed(&self) -> bool {
        self.mcp_allowed.load(Ordering::SeqCst)
    }

    fn note_tunnelled_request(&self, peer: Option<&str>) {
        self.tunnelled.fetch_add(1, Ordering::SeqCst);
        *self.last_peer.lock().unwrap() = peer.map(str::to_owned);
    }
}
