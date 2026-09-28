//! Putting the tunnel back when the proxy it went down with runs again.
//!
//! `backend.rs` takes the tunnel down with the proxy it fronts: a listener
//! cannot be re-pointed at a new one, and leaving it up forwards into a port
//! this daemon no longer owns. Nothing then brought it back until the daemon
//! restarted, so a proxy restart left remote access switched on with no
//! tunnel ([#1040]). This is the other half. The daemon's startup resume
//! goes on as a follower: it looks on the watcher's own [`PROXY_POLL`]
//! cadence, and once a tunnel has gone down with its proxy, the switch is on
//! and the proxy is running again, it arms from the switch — no code, the
//! same endpoint key, so the same ticket.
//!
//! What a re-arm will not do is most of the design:
//!
//! - It starts no proxy. A stopped proxy is a person's answer, so `turn_on`
//!   takes a re-arm's address from a running proxy, refuses otherwise, and
//!   does not call `ensure_running` for one.
//! - It stores no proxy key. When settling would mint one, `arm` refuses
//!   before it binds anything: a minted key locks the local proxy with nobody
//!   there to be told, and a proxy back up demanding none, with none stored,
//!   is what the documented way to turn local authentication off leaves.
//! - A `disable` beats it. It subscribes to `disables` before every read of
//!   the switch, as `resume` does, and `disable` says so again once its write
//!   of the switch is over. A look that read the switch before that write
//!   landed hears the second at its reservation and turns itself away, or
//!   reserved before it and is in the slot for the `disable` to take; one
//!   that read the switch after the write reads it off.
//! - It ends on the daemon's shutdown token. The daemon's teardown empties
//!   the serve slot while the proxy is still up, and a follower still looking
//!   then would arm into it.
//!
//! Each look ends in a [`Rearm`], which is what the daemon logs and what the
//! tests assert: a decision, rather than a tunnel that has not come back yet.
//!
//! [#1040]: https://github.com/mmogr/gglib/issues/1040

use gglib_runtime::proxy::ProxyStatus;
use tokio::sync::watch;
use tokio::time::{Instant, Interval, interval_at};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::RemoteOps;
use super::backend::PROXY_POLL;
use super::serve::Caller;
use super::serve_switch::CANCELLED_BY_DISABLE;
use super::types::EnableRequest;
use crate::error::GuiError;

/// What `turn_on` answers a re-arm when the proxy is not running.
pub(super) const PROXY_NOT_RUNNING: &str =
    "the proxy is not running, and putting the tunnel back does not start it";

/// What `arm` answers a re-arm when settling the proxy's key would mint one.
pub(super) const WOULD_MINT: &str = "the proxy demands no key and none is stored, and putting \
                                     the tunnel back does not mint one";

/// What the follower decided at one look, or that it has stopped looking.
#[derive(Debug, PartialEq, Eq)]
enum Rearm {
    /// Armed again from the switch, with no code, on the same endpoint key.
    Armed,
    /// Left down: remote access is switched off, or a `disable` landed while
    /// this was arming.
    SwitchOff,
    /// Left down for now: the proxy is not running. The next look asks again.
    ProxyNotRunning,
    /// Left down: arming would have minted a key for the proxy.
    WouldMint,
    /// Left down: arming failed, for this reason.
    Failed(String),
    /// The daemon's shutdown token is cancelled, and the follower has stopped.
    Ended,
}

impl RemoteOps {
    /// The daemon's startup resume, then the follower, until `shutdown` is
    /// cancelled.
    ///
    /// What the daemon spawns at start. The resume runs as it did on its own;
    /// the follower after it looks at each tunnel that goes down with its
    /// proxy, puts it back when it may, and logs what it decided each time.
    pub async fn resume_and_follow(&self, shutdown: CancellationToken) {
        // Before the resume, so a tunnel the resume arms and the proxy then
        // takes down is one the follower hears about.
        let lost = self.lost_with_proxy.subscribe();
        self.resume().await;
        self.follow(lost, &shutdown, say).await;
    }

    /// Look every [`PROXY_POLL`] for a tunnel `lost` says went down with its
    /// proxy, and hand what each look decided to `report`. The last thing
    /// reported is [`Rearm::Ended`].
    ///
    /// A lost tunnel is looked at again, at every poll, only while the answer
    /// is [`Rearm::ProxyNotRunning`], and a run of that answer is reported
    /// once, at its first look. Any other decision ends the run and is the
    /// tunnel's last, as the startup resume makes one attempt.
    async fn follow(
        &self,
        mut lost: watch::Receiver<u64>,
        shutdown: &CancellationToken,
        mut report: impl FnMut(Rearm),
    ) {
        let mut poll = interval_at(Instant::now() + PROXY_POLL, PROXY_POLL);
        // Whether the last look reported, or kept quiet about, "not running".
        let mut waiting = false;
        loop {
            let decision = tokio::select! {
                // First, so a cancelled token ends this the next time it is
                // polled, whatever else is ready — a look in progress too.
                biased;
                () = shutdown.cancelled() => break,
                decision = self.after_a_loss(&mut lost, &mut poll, shutdown) => decision,
            };
            if decision == Rearm::Ended {
                break;
            }
            let again = decision == Rearm::ProxyNotRunning;
            if again {
                // Still owed a look, at the next tick.
                lost.mark_changed();
            }
            // Said at the first look of a run, and not again at every poll for
            // as long as the proxy stays stopped.
            if !(again && waiting) {
                report(decision);
            }
            waiting = again;
        }
        report(Rearm::Ended);
    }

    /// Wait for a tick at which a lost tunnel is owed a look, and look.
    async fn after_a_loss(
        &self,
        lost: &mut watch::Receiver<u64>,
        poll: &mut Interval,
        shutdown: &CancellationToken,
    ) -> Rearm {
        loop {
            poll.tick().await;
            if lost.has_changed().unwrap_or(false) {
                lost.mark_unchanged();
                return self.rearm(shutdown).await;
            }
        }
    }

    /// One look at a tunnel that went down with its proxy: arm it again from
    /// the switch, when the switch is on and the proxy is running, and say
    /// what was decided.
    async fn rearm(&self, shutdown: &CancellationToken) -> Rearm {
        // Before the switch is read, as `resume` does: a `disable` from here
        // on is one `turn_on` sees, including one that lands between the read
        // and the reservation and finds nothing there to cancel.
        let disables = self.disables.subscribe();
        let settings = match self.core.settings().get().await {
            Ok(settings) => settings,
            Err(e) => return Rearm::Failed(format!("could not read settings: {e}")),
        };
        if settings.remote_enabled != Some(true) {
            return Rearm::SwitchOff;
        }
        let Some(serve) = settings.remote_serve else {
            return Rearm::Failed(
                "remote access is switched on but no serve options were stored".to_owned(),
            );
        };
        let request = EnableRequest {
            allow_mcp: serve.allow_mcp,
            relay: serve.relay,
            discovery: serve.discovery,
            // `Caller::Rearm` offers no code whatever this says.
            invite: false,
        };
        let caller = Caller::Rearm(shutdown.clone());
        let Err(refused) = self.turn_on(request, caller, disables).await else {
            return Rearm::Armed;
        };
        match refused {
            _ if shutdown.is_cancelled() => Rearm::Ended,
            GuiError::Conflict(m) if m == PROXY_NOT_RUNNING => Rearm::ProxyNotRunning,
            GuiError::Conflict(m) if m == WOULD_MINT => Rearm::WouldMint,
            GuiError::Conflict(m) if m == CANCELLED_BY_DISABLE => Rearm::SwitchOff,
            // Anything else, the proxy leaving while the tunnel bound among
            // them, stays owed while the proxy is down, so the next look
            // tries again once it runs rather than giving up on the tunnel.
            other => match self.proxy.status().await {
                ProxyStatus::Running { .. } => Rearm::Failed(other.to_string()),
                ProxyStatus::Stopped | ProxyStatus::Crashed => Rearm::ProxyNotRunning,
            },
        }
    }
}

/// The daemon's log of each decision `follow` reports, which is one line for
/// a run of [`Rearm::ProxyNotRunning`] however long the proxy stays stopped.
fn say(decision: Rearm) {
    match decision {
        Rearm::Armed => info!("remote tunnel put back in front of the proxy it went down with"),
        Rearm::SwitchOff => info!(
            "a remote tunnel went down with its proxy and remote access is switched off, so it \
             stays down"
        ),
        Rearm::ProxyNotRunning => {
            info!("a remote tunnel went down with its proxy, which is not running; waiting");
        }
        Rearm::WouldMint => warn!(
            "the remote tunnel was left down: the proxy came back demanding no key and none is \
             stored, and putting the tunnel back would have minted one, locking the local \
             proxy. `gglib remote enable` brings it back with a new key; `gglib remote \
             disable` switches remote access off"
        ),
        Rearm::Failed(reason) => warn!(
            "the remote tunnel was not put back in front of its proxy: {reason} — `gglib remote \
             enable` tries again; `gglib remote disable` switches remote access off"
        ),
        Rearm::Ended => debug!("no longer following the proxy for the remote tunnel"),
    }
}

#[cfg(test)]
#[path = "serve_rearm_tests.rs"]
mod serve_rearm_tests;

#[cfg(test)]
#[path = "serve_rearm_look_tests.rs"]
mod serve_rearm_look_tests;

#[cfg(test)]
#[path = "serve_rearm_disable_tests.rs"]
mod serve_rearm_disable_tests;

#[cfg(test)]
#[path = "serve_rearm_once_tests.rs"]
mod serve_rearm_once_tests;

#[cfg(test)]
#[path = "serve_rearm_daemon_tests.rs"]
mod serve_rearm_daemon_tests;
