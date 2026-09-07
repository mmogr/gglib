#![doc = include_str!("README.md")]

mod backend;
mod connect;
mod connect_watch;
mod first_contact;
mod gateway;
mod key;
mod pairing;
mod pairing_string;
mod redeem;
mod rotation;
mod serve;
mod slot;
mod stored_pairing;
mod teardown;
mod types;

pub use gateway::RemoteGateway;
pub use types::{
    ConnectRequest, ConnectSnapshot, Connected, EnableRequest, Enabled, RemoteStatusSnapshot,
};

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use gglib_core::ports::AppEventEmitter;
use gglib_core::services::AppCore;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::proxy::ProxyOps;
use connect::LiveConnect;
use slot::Slot;

/// How long `serve` may wait for the endpoint to reach a relay before the
/// ticket is minted. The ticket is about to be shown to a person and copied
/// once, so a few seconds here buys a ticket that carries the relay path a
/// peer behind a strict NAT needs. Expiry is not an error.
const WAIT_ONLINE: Duration = Duration::from_secs(10);

/// How long a teardown lets in-flight requests finish before cutting them.
const DRAIN: Duration = Duration::from_secs(5);

/// One live serve side and the tasks that keep it honest.
///
/// Generic over the handle so [`backend::take_if_ours`] is testable at all.
struct Live<H = modelpipe::ServeHandle> {
    handle: Arc<H>,
    /// Cancelled when this tunnel goes down, which is what stops both the
    /// rotation poll and the watcher following the proxy it fronts. One
    /// token for both: neither outlives the tunnel they belong to, and a
    /// second field would only be one more thing to forget.
    cancel: CancellationToken,
    /// Which session the gateway holds a pairing and an `/mcp` grant for on
    /// this tunnel's behalf, from
    /// [`RemoteGateway::begin_session`](gateway::RemoteGateway::begin_session).
    /// Presented at teardown so a drain that took five seconds cannot clear
    /// a session that started while it was draining.
    epoch: u64,
}

/// The remote tunnel's lifecycle: both sides of ADR 0012.
///
/// Off by default and never persisted: `enable` arms the serve side and
/// `connect` the connect side for this daemon only, and nothing brings
/// either back on a restart. The two are independent — a machine can be
/// both the desktop for one peer and the laptop to another.
pub struct RemoteOps {
    proxy: Arc<ProxyOps>,
    core: Arc<AppCore>,
    gateway: Arc<RemoteGateway>,
    emitter: Arc<dyn AppEventEmitter>,
    live: Arc<Mutex<Slot<Live>>>,
    /// Shared with the task that watches the connection, which is why it is
    /// an `Arc` where `live` is not.
    live_connect: Arc<Mutex<Slot<LiveConnect>>>,
    /// Names each `connect`, so a teardown that took the slot from one is
    /// something that dial can see when it comes back, and a watcher that
    /// outlived its connection cannot clear the next one.
    connect_generation: AtomicU64,
    /// The same, for `enable`.
    enable_generation: AtomicU64,
}

impl RemoteOps {
    /// Build the ops over the gateway the proxy was handed.
    pub fn new(
        proxy: Arc<ProxyOps>,
        core: Arc<AppCore>,
        gateway: Arc<RemoteGateway>,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Self {
        Self {
            proxy,
            core,
            gateway,
            emitter,
            live: Arc::new(Mutex::new(Slot::Empty)),
            live_connect: Arc::new(Mutex::new(Slot::Empty)),
            connect_generation: AtomicU64::new(0),
            enable_generation: AtomicU64::new(0),
        }
    }

    /// The gateway this owns, for the service graph to hand to `ProxyOps`.
    #[must_use]
    pub fn gateway(&self) -> Arc<RemoteGateway> {
        Arc::clone(&self.gateway)
    }

    /// A snapshot for the status surface: both sides, and what settings
    /// remember of the last pairing (by fingerprint, never the ticket).
    pub async fn status(&self) -> RemoteStatusSnapshot {
        // Both answers come off one record, which is what makes them
        // agree: a key is held *for* the machine the fingerprint names, and
        // there is no longer a shape in which they can describe two.
        let stored = self
            .core
            .settings()
            .get()
            .await
            .ok()
            .and_then(|s| s.remote_pairing);
        let stored_ticket_fingerprint = stored.as_ref().and_then(stored_pairing::fingerprint);
        let has_remote_key = stored.is_some();
        let connected = self.connect_snapshot().await;
        let live = self.live.lock().await;
        let mut snapshot = RemoteStatusSnapshot {
            enabled: live.full().is_some(),
            pairing_active: self.gateway.pairing.active(),
            paired: self.gateway.paired(),
            mcp_allowed: self.gateway.mcp_allowed_now(),
            tunnelled_requests: self.gateway.tunnelled_requests(),
            last_tunnelled_ms: self.gateway.last_tunnelled_ms(),
            last_peer: self.gateway.last_peer(),
            connected,
            stored_ticket_fingerprint,
            has_remote_key,
            ..RemoteStatusSnapshot::default()
        };
        if let Some(Live { handle, .. }) = live.full() {
            snapshot.ticket_fingerprint = Some(handle.ticket().fingerprint());
            snapshot.path = Some(handle.status().as_str().to_owned());
            snapshot.peers = handle
                .peers()
                .into_iter()
                .map(|peer| (peer.fingerprint, peer.path.as_str().to_owned()))
                .collect();
        }
        snapshot
    }
}

impl RemoteGateway {
    /// [`RemoteGatewayPort::mcp_allowed`](gglib_core::ports::RemoteGatewayPort::mcp_allowed),
    /// reachable without importing the trait.
    fn mcp_allowed_now(&self) -> bool {
        gglib_core::ports::RemoteGatewayPort::mcp_allowed(self)
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "enable_tests.rs"]
mod enable_tests;

#[cfg(test)]
#[path = "serve_watch_tests.rs"]
mod serve_watch_tests;
