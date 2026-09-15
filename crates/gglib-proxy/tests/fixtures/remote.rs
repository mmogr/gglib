//! A stand-in for the tunnel's owner, for the proxy's remote-tunnel tests.
//!
//! Implements [`RemoteGatewayPort`] and records what the proxy tells it — the
//! same contract `gglib-app-services` implements over its real session,
//! reduced to what these tests need to observe. Pairing is not part of it:
//! the tunnel edge answers a pairing request itself, and nothing reaches the
//! proxy.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gglib_core::ports::RemoteGatewayPort;

/// The stub. Fields are read by the tests; the port methods write them.
pub(crate) struct StubGateway {
    mcp_allowed: AtomicBool,
    /// Every request the proxy reported as tunnelled.
    pub(crate) tunnelled: AtomicUsize,
    /// The peer fingerprint on the most recent tunnelled request.
    pub(crate) last_peer: Mutex<Option<String>>,
    /// The device name the edge said admitted the most recent tunnelled
    /// request, when it named one.
    pub(crate) last_device: Mutex<Option<String>>,
}

impl StubGateway {
    pub(crate) fn new(mcp_allowed: bool) -> Self {
        Self {
            mcp_allowed: AtomicBool::new(mcp_allowed),
            tunnelled: AtomicUsize::new(0),
            last_peer: Mutex::new(None),
            last_device: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for StubGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubGateway").finish_non_exhaustive()
    }
}

impl RemoteGatewayPort for StubGateway {
    fn mcp_allowed(&self) -> bool {
        self.mcp_allowed.load(Ordering::SeqCst)
    }

    fn note_tunnelled_request(&self, peer: Option<&str>, device: Option<&str>) {
        self.tunnelled.fetch_add(1, Ordering::SeqCst);
        *self.last_peer.lock().unwrap() = peer.map(str::to_owned);
        *self.last_device.lock().unwrap() = device.map(str::to_owned);
    }
}
