#![doc = include_str!("README.md")]

mod backend;
mod connect;
mod connect_watch;
mod device_keys;
mod device_line;
mod device_runs;
mod device_view;
mod devices;
mod enrolment;
mod far_daemon;
mod gateway;
mod identity;
mod invite_watch;
pub(crate) mod key;
mod pairing;
mod resume_wait;
mod roster;
mod rotation;
mod serve;
mod serve_arm;
mod serve_rearm;
mod serve_switch;
mod slot;
mod status;
mod stored_pairing;
mod teardown;
mod types;
mod wire;
mod wire_exchange;

pub use gateway::RemoteGateway;
pub use types::{EnableRequest, Enabled, JoinRequest, Joined, OfferedPairing};
pub use wire::{RemoteConnection, RemoteDevice, RemoteForgotten, RemotePeer, RemoteStatus};
pub use wire_exchange::{
    RemoteEnableBody, RemoteEnableResponse, RemoteJoinBody, RemoteJoinResponse,
};

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use gglib_core::ports::AppEventEmitter;
use gglib_core::services::AppCore;
use tokio::sync::{Mutex, watch};
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

/// How long `enable` and `invite` wait for the daemon's own startup resume
/// before answering as though it were not there.
///
/// Budgeted from what a resume does: start the proxy, which nothing here
/// bounds, wait up to [`WAIT_ONLINE`] for a relay, and, on a machine with no
/// key in settings yet, wait out the five-second window a freshly minted key
/// needs. A first `enable` that failed or was killed before it minted one
/// leaves exactly that machine. Twenty covers those fifteen with five to
/// spare for the proxy, and twenty plus the fifteen a caller's own arm can
/// take after it stays inside the 45 seconds the CLI gives
/// `POST /api/remote/enable`.
const WAIT_OUT_RESUME: Duration = Duration::from_secs(20);

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
/// the same endpoint key, so paired devices keep working — and again once a
/// proxy the tunnel went down with runs again, unless arming would mint a
/// key (`serve_rearm.rs`). The connect side
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
    /// Names each `join`, so a teardown that took the slot from one is
    /// something that dial can see when it comes back, and a watcher that
    /// outlived its connection cannot clear the next one.
    connect_generation: AtomicU64,
    /// The same, for `enable`.
    enable_generation: AtomicU64,
    /// Serialises read-modify-write over the device roster *and its key
    /// file*, which are written together and must not interleave.
    ///
    /// `remote_devices` is written whole, from a copy each writer reads
    /// first — so an `invite` pushing a row and a `forget` retaining one
    /// would each read the roster the other had not yet written, and one
    /// change would vanish. Separate from `live` because the settings write
    /// is slow and `invite` must not hold the serve slot across it.
    ///
    /// It covers the roster's own writers and nothing else, and needs to: any
    /// other settings write — `disable`, `join`, the settings form, a proxy
    /// minting its key, `gglib config settings set` or `reset` in another
    /// process — goes through `SettingsRepository::modify`, one transaction
    /// that rewrites only the fields it changed.
    roster: Arc<Mutex<()>>,
    /// True from the first line of the daemon's startup `resume` to its last:
    /// a wider span than the reservation that resume takes, because
    /// `turn_on` starts the proxy before it reserves anything. `enable` and
    /// `invite` wait on it; `resume_wait.rs` has the rest.
    resuming: watch::Sender<bool>,
    /// Bumped twice by every `disable`, so a call that subscribed earlier can
    /// tell that the person changed their mind since: an `enable` or `invite`
    /// waiting out a resume, or an `enable`, the startup resume or the
    /// daemon's re-arm (`serve_rearm.rs`) on its way to arming. Once before it
    /// writes the switch off, and again once that write is over, for a call
    /// that subscribed between the two: an arm from the switch can subscribe
    /// then and still read the switch before the write lands.
    disables: watch::Sender<u64>,
    /// Bumped by the watcher following the proxy each time it takes its
    /// tunnel down because the proxy went away, and read by the daemon's
    /// follower, which puts that tunnel back once the proxy runs again
    /// (`serve_rearm.rs`).
    lost_with_proxy: watch::Sender<u64>,
    /// The file the device keys are kept in, or `None` for the one beside
    /// the endpoint identity, where a daemon keeps them. A test names its
    /// own: in a debug build the default is the checkout's `data/`, which is
    /// also the installed daemon's.
    device_keys: Option<PathBuf>,
    /// The directory the keys this machine joins other machines with are
    /// kept in, or `None` for `<data root>/data/remote_join`, where a daemon
    /// keeps them. A test names its own with
    /// [`with_join_keys`](Self::with_join_keys), for the reason it names a
    /// device key file.
    join_keys: Option<PathBuf>,
}

impl RemoteOps {
    /// Build the ops over the gateway the proxy was handed, keeping device
    /// keys in `device_keys`, or beside the endpoint identity when `None`.
    pub fn new(
        proxy: Arc<ProxyOps>,
        core: Arc<AppCore>,
        gateway: Arc<RemoteGateway>,
        emitter: Arc<dyn AppEventEmitter>,
        device_keys: Option<PathBuf>,
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
            resuming: watch::channel(false).0,
            disables: watch::channel(0).0,
            lost_with_proxy: watch::channel(0).0,
            device_keys,
            join_keys: None,
        }
    }

    /// The gateway this owns, for the service graph to hand to `ProxyOps`.
    #[must_use]
    pub fn gateway(&self) -> Arc<RemoteGateway> {
        Arc::clone(&self.gateway)
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

#[cfg(test)]
#[path = "enable_wait_tests.rs"]
mod enable_wait_tests;

#[cfg(test)]
#[path = "serve_resume_tests.rs"]
mod serve_resume_tests;

#[cfg(test)]
#[path = "enable_race_tests.rs"]
mod enable_race_tests;
