//! Which local address the tunnel fronts, and noticing when it stops being
//! one.
//!
//! `enable` reads the proxy's bound address exactly once, and both halves of
//! this file are about that single read: one turns a *bind* address into
//! something `modelpipe::serve` will dial, and the other takes the tunnel
//! down when that address stops meaning the proxy.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_runtime::proxy::ProxyStatus;
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::gateway::RemoteGateway;
use super::slot::Slot;
use super::{DRAIN, Live, RemoteOps};
use crate::error::GuiError;
use crate::proxy::ProxyOps;

/// Where the tunnel dials this machine's proxy, and on what terms.
pub(super) struct Backend {
    /// The backend URL handed to `modelpipe::serve`.
    pub(super) url: String,
    /// Whether that URL names an address on the operator's own network,
    /// which modelpipe will not dial unless it is told to expect one.
    pub(super) allow_private: bool,
}

impl Backend {
    /// The backend a proxy bound to `addr` is reached at.
    ///
    /// A bind address and a dial address are not the same thing, and
    /// modelpipe screens the address it dials (`locality::admits`) against a
    /// rule the proxy's bind never had to satisfy. Two cases differ:
    ///
    /// * A **wildcard** bind (`0.0.0.0`, `::`) names no host at all, so
    ///   modelpipe refuses it whatever it is told — on Linux, dialling it
    ///   reaches loopback, so admitting it would be an accident rather than
    ///   a decision. A proxy on the wildcard is listening on loopback as
    ///   well, so the loopback literal of the same family is what it meant.
    ///   The port is kept, which is the whole point of rewriting rather than
    ///   guessing at 8080.
    /// * A **LAN** bind (`192.168.…`, `10.…`, `fd00::…`) names a real
    ///   interface that the loopback literal would not reach, so it is
    ///   dialled as written — with the one flag modelpipe requires before it
    ///   will dial the operator's own network. That widens nothing else: the
    ///   URL carries a literal address, so the flag can only ever readmit
    ///   the address the proxy is already on.
    ///
    /// Everything else is passed through untouched and modelpipe decides.
    /// Link-local and public addresses stay refused however the flag is set,
    /// and this must not try to talk its way around that: a proxy bound to a
    /// routable address would be re-exporting a server this machine does not
    /// own, which is the case the rule exists for.
    pub(super) fn at(addr: SocketAddr) -> Self {
        let ip = match addr.ip() {
            IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            other => other,
        };
        Self {
            // `SocketAddr`'s `Display` brackets an IPv6 literal, which is
            // what a URL authority needs — and what modelpipe strips back
            // off the host before it resolves, so `http://[::1]:8080` is the
            // spelling that works rather than the one that looks safe.
            url: format!("http://{}", SocketAddr::new(ip, addr.port())),
            allow_private: is_private(ip),
        }
    }
}

/// Whether an address is one modelpipe classifies as private — RFC 1918 or
/// `fc00::/7`.
///
/// A mirror of `locality::classify`, whose verdict is the one that actually
/// decides; this side only has to predict it well enough to set the flag.
/// IPv4-mapped IPv6 is unwrapped first for the reason it is unwrapped there:
/// `::ffff:192.168.1.5` is a LAN address wearing an IPv6 hat, and every
/// per-family check answers the wrong question about it.
fn is_private(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.is_private(),
        // fc00::/7, unique local. Matched by prefix because
        // `Ipv6Addr::is_unique_local` is still unstable.
        IpAddr::V6(v6) => v6.segments()[0] & 0xFE00 == 0xFC00,
    }
}

/// Refuse a tunnel whose proxy went away while it was binding, and let the
/// half-built listener go.
///
/// Nothing watches the proxy across the span this covers, which is why it
/// exists. `enable` reserves the serve slot rather than holding its lock, so
/// `status` keeps answering — but [`follow_proxy`]'s watcher is spawned only
/// once the tunnel is installed, because [`take_if_ours`] passes over a
/// reservation and `watch_proxy` looks exactly once. So a proxy that exits
/// between the address being read and the install is seen by nobody, and the
/// caller is handed a pairing string for a tunnel fronting a released port,
/// with nothing anywhere saying why the code never worked. Failing closed is
/// the answer; reporting success is not. Asked of `status()` rather than the
/// exit channel, which does not see the exits nothing publishes
/// ([`PROXY_POLL`]).
///
/// Asked *before* [`key::Settled::commit`](super::key::Settled::commit), so
/// on a first enable the answer can be a settings-cache window old by the
/// time `enable` returns — the deliberate half of a trade that commit
/// documents: asking later leaves a minted key behind for a refused tunnel.
///
/// # Errors
///
/// `Internal`, naming the state the proxy was found in.
pub(super) async fn refuse_if_gone(
    proxy: &ProxyOps,
    backend: &Backend,
    handle: &modelpipe::ServeHandle,
) -> Result<(), GuiError> {
    let status = proxy.status().await;
    if still_fronting(&status, backend) {
        return Ok(());
    }
    // Nothing can be in flight: the ticket has not left `enable`.
    if !handle.shutdown_timeout(DRAIN).await {
        warn!("the tunnel that could not be enabled was slow to go away");
    }
    Err(GuiError::Internal(format!(
        "the local proxy this tunnel would front went away while the tunnel was starting \
         ({status}); remote access was not enabled"
    )))
}

/// Follow the proxy this tunnel fronts, and take the tunnel down when it
/// goes away.
///
/// `enable` captures the proxy's address once and `modelpipe::serve` holds
/// it for the listener's whole life: a running listener cannot be re-pointed
/// at another port, and re-serving would mint a fresh identity — a new
/// ticket, and every paired machine unpaired. So the only honest answer to
/// the proxy exiting is to stop fronting it.
///
/// Leaving it up is worse than having no tunnel. The port stops being this
/// daemon's the moment the proxy lets go of it, another local process may
/// bind it, and modelpipe 0.3.0 forwards `Authorization` verbatim — so
/// whatever answers there next is handed the tunnelled request *and* the
/// gglib key that came with it.
///
/// Two mechanisms, because one of them has a hole. The exit channel is the
/// fast path and covers the ordinary exits; the poll is what covers the ones
/// nothing announces. The receiver must be taken before the address is read:
/// it publishes only exits, never starts, so a subscription made afterwards
/// would silently miss a proxy that fell over in between.
pub(super) fn follow_proxy(
    ops: &RemoteOps,
    handle: &Arc<modelpipe::ServeHandle>,
    exit: watch::Receiver<ProxyStatus>,
    cancel: CancellationToken,
    backend: Backend,
) {
    tokio::spawn(watch_proxy(
        Arc::clone(&ops.live),
        Arc::clone(handle),
        Arc::clone(&ops.gateway),
        Arc::clone(&ops.emitter),
        Arc::clone(&ops.proxy),
        exit,
        cancel,
        backend,
    ));
}

/// How often the watcher asks the supervisor what the proxy is doing, on top
/// of the exit channel it is subscribed to.
///
/// The channel is not enough on its own: it is published from inside the
/// proxy task, *after* the serve future returns, so a task that panicked or
/// that `ProxySupervisor::stop` aborted on its five-second timeout publishes
/// nothing at all. `status()` sees all three — it reads `is_finished()` and a
/// taken handle rather than a message — so it is what closes the case the
/// channel leaves open. Five seconds is a lock and an `is_finished()`, and it
/// bounds how long a released port can be fronted.
const PROXY_POLL: Duration = Duration::from_secs(5);

/// The task [`follow_proxy`] spawns: wait until the proxy stops being the one
/// this tunnel was built in front of, then undo `enable`.
#[expect(
    clippy::too_many_arguments,
    reason = "the pieces of `RemoteOps` this outlives, plus what it is \
              watching; bundling them into a struct used once would hide \
              rather than reduce them"
)]
async fn watch_proxy(
    live: Arc<Mutex<Slot<Live>>>,
    handle: Arc<modelpipe::ServeHandle>,
    gateway: Arc<RemoteGateway>,
    emitter: Arc<dyn AppEventEmitter>,
    proxy: Arc<ProxyOps>,
    mut exit: watch::Receiver<ProxyStatus>,
    cancel: CancellationToken,
    backend: Backend,
) {
    if !until_gone(&proxy, &mut exit, &cancel, &backend).await {
        return;
    }
    let taken = take_if_ours(&mut *live.lock().await, &handle);
    let Some(live) = taken else { return };
    // The same order `disable` ends a session in, and for the same reason —
    // see [`teardown::take_down`].
    super::teardown::take_down(live, &gateway).await;
    info!("remote tunnel disabled with the proxy it fronted");
    emitter.emit(AppEvent::remote_disabled());
}

/// Wait until the proxy stops being the one this tunnel dials.
///
/// `true` when it is gone and the tunnel has to come down with it; `false`
/// when this watcher is the thing that ended — `disable` cancelled the
/// shared token, or the supervisor's channel closed because the process
/// itself is coming down — in which case there is nothing left to undo and
/// racing `disable` for the same handle would be the only thing achieved.
///
/// Two arms, because one of them has a hole. The exit channel is the fast
/// path; the poll is what covers the exits nothing publishes.
async fn until_gone(
    proxy: &ProxyOps,
    exit: &mut watch::Receiver<ProxyStatus>,
    cancel: &CancellationToken,
    backend: &Backend,
) -> bool {
    let mut poll = tokio::time::interval_at(tokio::time::Instant::now() + PROXY_POLL, PROXY_POLL);
    loop {
        tokio::select! {
            () = cancel.cancelled() => return false,
            changed = exit.changed() => {
                // The sender outlives every proxy run, so a closed channel
                // means the process itself is coming down.
                if changed.is_err() {
                    return false;
                }
                let status = exit.borrow_and_update().clone();
                if !still_fronting(&status, backend) {
                    warn!(%status, "the proxy the remote tunnel fronts exited");
                    return true;
                }
            }
            _ = poll.tick() => {
                let status = proxy.status().await;
                if !still_fronting(&status, backend) {
                    warn!(%status, "the proxy the remote tunnel fronts is no longer there");
                    return true;
                }
            }
        }
    }
}

/// Take the tunnel out of `slot`, but only when it is the one `mine` serves.
///
/// Identity rather than a generation counter: the only tunnel this watcher
/// may take down is the one it was spawned beside, and `Arc::ptr_eq` says
/// exactly that with no second field to keep in step. Three ways it can fail
/// to match matter. A `disable` that got here first leaves nothing at all; a
/// `disable` followed by a fresh `enable` leaves a *different* tunnel —
/// taking that one down would unpair every machine paired with it, over a
/// proxy exit that had nothing to do with it; and a slot still being filled
/// is passed over by [`Slot::take_if`], which is why this watcher is spawned
/// after the install rather than before it.
///
/// Generic over the handle only because a real `modelpipe::ServeHandle`
/// exists nowhere but in front of a bound listener; nothing here looks
/// inside one.
pub(super) fn take_if_ours<H>(slot: &mut Slot<Live<H>>, mine: &Arc<H>) -> Option<Live<H>> {
    slot.take_if(|l| Arc::ptr_eq(&l.handle, mine))
}

/// Whether a proxy in this state is still the one this tunnel dials.
fn still_fronting(status: &ProxyStatus, backend: &Backend) -> bool {
    match status {
        // The address is compared, not assumed: a proxy that went away and
        // came back on another port is not this tunnel's backend, even
        // though it is running. The comparison goes through the same
        // rewrite `enable` used, so a proxy that rebound the wildcard is
        // recognised as the same backend rather than as a new one.
        ProxyStatus::Running { address } => Backend::at(*address).url == backend.url,
        // `POST /api/proxy/stop` publishes the first and a proxy task that
        // fell over publishes the second. Both are equally not ours any
        // more, and only reacting to the crash would leave the deliberate
        // stop — the one a person just asked for — forwarding into the dark.
        ProxyStatus::Stopped | ProxyStatus::Crashed => false,
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod backend_tests;
