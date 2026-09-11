#![doc = include_str!("README.md")]

mod backend;
mod connect;
mod connect_watch;
mod device_keys;
mod devices;
mod enrolment;
mod first_contact;
mod gateway;
mod key;
mod pairing;
mod pairing_string;
mod redeem;
mod roster;
mod rotation;
mod serve;
mod serve_switch;
mod slot;
mod stored_pairing;
mod teardown;
mod types;

pub use gateway::RemoteGateway;
pub use types::{
    ConnectRequest, ConnectSnapshot, Connected, DeviceView, EnableRequest, Enabled, OfferedPairing,
    RemoteStatusSnapshot,
};

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use gglib_core::ports::AppEventEmitter;
use gglib_core::services::AppCore;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::warn;

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
/// Off by default, and the two sides differ in what a restart does. The
/// serve side is a switch: `remote_enabled` is persisted and the daemon
/// arms the tunnel again at startup with the flags it was enabled with, on
/// the same endpoint key, so paired devices keep working. The connect side
/// is not: `join` binds a loopback port for this daemon only, and the
/// laptop dials again from the pairing it stored. The two are independent —
/// a machine can be both the desktop for one peer and the laptop to
/// another.
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
    /// Serialises read-modify-write over the device roster *and its key
    /// file*, which are written together and must not interleave.
    ///
    /// `SettingsService::update` is load, merge, save with no lock of its
    /// own, and `remote_devices` is written whole — so an `invite` pushing a
    /// row and a `forget` retaining one would each read the roster the other
    /// had not yet written, and one change would vanish. Separate from
    /// `live` because the settings write is slow and `invite` must not hold
    /// the serve slot across it.
    ///
    /// **It covers the roster's own writers and nothing else.** Every other
    /// caller of `SettingsService::update` — `disable` clearing the switch,
    /// `connect` storing a pairing, the settings form, `gglib config settings
    /// set` — loads and saves the whole record without taking this, so one
    /// landing across a roster write still drops a row. That is a property of
    /// the settings service rather than of this lock, and it predates the
    /// roster; what the roster adds is the first writer driven by traffic
    /// rather than by a person (`last_seen`, at most once a minute per
    /// device), which makes the collision likelier than it was. The fix
    /// belongs in `SettingsService`, not here.
    roster: Arc<Mutex<()>>,
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
            roster: Arc::new(Mutex::new(())),
        }
    }

    /// The gateway this owns, for the service graph to hand to `ProxyOps`.
    #[must_use]
    pub fn gateway(&self) -> Arc<RemoteGateway> {
        Arc::clone(&self.gateway)
    }

    /// A snapshot for the status surface: both sides, what settings remember
    /// of the last pairing (by fingerprint, never the ticket), and the
    /// roster.
    ///
    /// The device list rides this call rather than having one of its own
    /// because its read is already paid for: the roster is a settings field,
    /// and the record is read here regardless. A surface that re-reads the
    /// status gets the list with it, at no second read, and the list can never
    /// disagree with the `enabled` beside it about whether anything is being
    /// admitted.
    pub async fn status(&self) -> RemoteStatusSnapshot {
        // Every settings answer below — the switch, the stored pairing and
        // the roster — comes off one record, which is what makes them agree:
        // a key is held *for* the machine the fingerprint names, and there is
        // no longer a shape in which they can describe two.
        //
        // Swallowed because `status` is what someone runs *because* something
        // is wrong and must not itself fail — but logged, because the
        // fallback is an empty roster, and "no device has been paired with
        // this machine" is a confident wrong answer on the one surface a
        // person opens to decide what to revoke. `RemoteOps::list` returns
        // the error; this is the trade the two make differently, on purpose.
        let settings = match self.core.settings().get().await {
            Ok(settings) => Some(settings),
            Err(e) => {
                warn!("could not read settings for remote status; reporting none: {e}");
                None
            }
        };
        let remote_enabled = settings
            .as_ref()
            .is_some_and(|s| s.remote_enabled == Some(true));
        let (roster, stored) = settings
            .map(|s| (s.remote_devices.unwrap_or_default(), s.remote_pairing))
            .unwrap_or_default();
        let stored_ticket_fingerprint = stored.as_ref().and_then(stored_pairing::fingerprint);
        let has_remote_key = stored.is_some();
        let connected = self.connect_snapshot().await;
        // Before the lock: this touches the filesystem — `create_dir_all` on
        // the data directory — and `status` is the call everything else waits
        // behind. Nothing under the serve slot should be doing IO that has
        // nothing to do with the slot.
        let identity_path = key::identity_path()
            .ok()
            .flatten()
            .map(|p| p.display().to_string());
        let live = self.live.lock().await;
        // Under the slot, like `list`'s: what the edge holds and whether the
        // tunnel is up have to be read at one instant or a row can come back
        // "not admitted" from a session that had already gone.
        let admitting = live.full().map(|l| l.handle.token_names());
        let mut snapshot = RemoteStatusSnapshot {
            devices: devices::viewed(roster, admitting.as_deref()),
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
            remote_enabled,
            identity_path,
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

#[cfg(test)]
#[path = "serve_invite_tests.rs"]
mod serve_invite_tests;
