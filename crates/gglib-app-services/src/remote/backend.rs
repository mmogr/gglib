//! Noticing when the tunnel stops fronting the proxy it was built for.
//!
//! `enable` reads the proxy's bound address exactly once and hands it to
//! [`BackendUrl::at`], which turns a *bind* address into one
//! `modelpipe::serve` will dial — rewriting a wildcard to the loopback
//! literal of its own family and carrying the permission a LAN address
//! needs. The locality rule travels with the URL, so this file does the
//! other half: taking the tunnel down when that address stops meaning the
//! proxy.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_runtime::proxy::ProxyStatus;
use modelpipe::BackendUrl;
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::gateway::RemoteGateway;
use super::slot::Slot;
use super::{DRAIN, Live, RemoteOps};
use crate::error::GuiError;
use crate::proxy::ProxyOps;

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
    backend: &BackendUrl,
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
/// daemon's the moment the proxy lets go of it, and another local process
/// may bind it. `arm` sets `backend_auth`, so the edge presents *this
/// daemon's own proxy key* upstream on every admitted request — whatever
/// answers on that port next is handed the tunnelled request and the key
/// that opens this machine's proxy. (Left unset, modelpipe forwards the
/// client's `Authorization` verbatim instead, which is the same hazard
/// wearing the device's credential rather than the daemon's.)
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
    backend: BackendUrl,
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
    backend: BackendUrl,
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
    backend: &BackendUrl,
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
fn still_fronting(status: &ProxyStatus, backend: &BackendUrl) -> bool {
    match status {
        // The address is compared, not assumed: a proxy that went away and
        // came back on another port is not this tunnel's backend, even
        // though it is running. The comparison goes through the same
        // rewrite `enable` used, so a proxy that rebound the wildcard is
        // recognised as the same backend rather than as a new one. The
        // whole value is compared: `at` derives the permission from the
        // address, so no two values can agree on the URL and differ on the
        // permission.
        ProxyStatus::Running { address } => BackendUrl::at(*address) == *backend,
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
