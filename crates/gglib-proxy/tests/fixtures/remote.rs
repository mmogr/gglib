//! A stand-in for the tunnel's owner, for the proxy's remote-tunnel tests,
//! and the headers the tunnel edge marks a named device's request with.
//!
//! The stand-in implements [`RemoteGatewayPort`] and records what the proxy
//! tells it — the same contract `gglib-app-services` implements over its real
//! session, reduced to what these tests need to observe. Pairing is not part
//! of it: the tunnel edge answers a pairing request itself, and nothing
//! reaches the proxy.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use gglib_core::ports::RemoteGatewayPort;
use reqwest::RequestBuilder;

use super::tunnel::DEVICE;

/// A request as the tunnel edge marks one from a named device.
pub(crate) fn from_device(request: RequestBuilder) -> RequestBuilder {
    request
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-device", DEVICE)
}

/// The stub. Fields are read by the tests; the port methods write them.
pub(crate) struct StubGateway {
    /// Every request the proxy reported as tunnelled.
    pub(crate) tunnelled: AtomicUsize,
    /// The peer fingerprint on the most recent tunnelled request.
    pub(crate) last_peer: Mutex<Option<String>>,
    /// The device name the edge said admitted the most recent tunnelled
    /// request, when it named one.
    pub(crate) last_device: Mutex<Option<String>>,
    /// Whether the stand-in's session is live.
    pub(crate) serving: bool,
}

impl StubGateway {
    /// A tunnel owner with no live session.
    pub(crate) fn new() -> Self {
        Self {
            tunnelled: AtomicUsize::new(0),
            last_peer: Mutex::new(None),
            last_device: Mutex::new(None),
            serving: false,
        }
    }

    /// A tunnel owner whose session is live.
    pub(crate) fn serving() -> Self {
        Self {
            serving: true,
            ..Self::new()
        }
    }
}

impl std::fmt::Debug for StubGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubGateway").finish_non_exhaustive()
    }
}

impl RemoteGatewayPort for StubGateway {
    fn serving(&self) -> bool {
        self.serving
    }

    fn note_tunnelled_request(&self, peer: Option<&str>, device: Option<&str>) {
        self.tunnelled.fetch_add(1, Ordering::SeqCst);
        *self.last_peer.lock().unwrap() = peer.map(str::to_owned);
        *self.last_device.lock().unwrap() = device.map(str::to_owned);
    }
}
