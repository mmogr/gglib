//! The launch guard: who owns a spawned llama-server before the queue does.
//!
//! Split from `launch.rs` because it is a self-contained unit with its own
//! tests, and because the invariant it encodes deserves to be read on its own
//! rather than found halfway through a launch.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::process::core::GuiProcessCore;

/// How often a launch checks that the server it is waiting for still exists.
///
/// Short enough that a startup crash is reported in seconds rather than at the
/// launch deadline, long enough that the check costs nothing next to the load
/// it is watching.
pub(super) const LIVENESS_TICK: std::time::Duration = std::time::Duration::from_secs(2);

/// Owns a spawned llama-server until the admission queue takes it.
///
/// Between `GuiProcessCore::spawn` and `AdmissionQueue::install` the child is
/// registered in `processes` but invisible to every path that could stop it:
/// eviction and recycling are gated on the queue returning a resident, and
/// `cleanup_dead` reaps only processes that have already exited. Anything that
/// ends the launch in that window leaks a live server holding VRAM, and
/// `spawn`'s "already running" guard then refuses every retry until the daemon
/// restarts.
///
/// `Drop` rather than an error arm because the launch runs inside a
/// `tokio::time::timeout`: overrunning it *drops* the future mid-await, so no
/// `?`, no `if let Err(..)`, and no `match` on the way out will run. Drop is
/// the only hook cancellation cannot skip, which also means a step added to
/// the launch later inherits the cleanup for free.
pub(super) struct SpawnedChild {
    core: Arc<RwLock<GuiProcessCore>>,
    model_id: u32,
    /// The process this guard owns, as distinct from the *name* it is filed
    /// under. See [`Self::arm`].
    pid: u32,
    armed: bool,
}

impl SpawnedChild {
    /// Arm against the process currently tracked for `model_id`.
    ///
    /// The pid is captured now and checked again at kill time, because the id
    /// is a name that gets reused: the failure path frees the slot
    /// synchronously while this guard's kill is still queued, so a waiting
    /// request can relaunch the same model and register a new child under the
    /// same id before the kill runs. Killing by name would then stop a
    /// healthy server that had just replaced the one this guard owned.
    ///
    /// A pid is not a *durable* identity — the OS may recycle one — but it is
    /// durable enough here: a false match needs the whole pid space to wrap
    /// inside the few seconds between a failed launch and its cleanup. A
    /// monotonic spawn generation on `ServerInfo` would be immune by
    /// construction rather than by improbability, if that struct is ever
    /// touched for another reason.
    pub(super) fn arm(core: &Arc<RwLock<GuiProcessCore>>, model_id: u32, pid: u32) -> Self {
        Self {
            core: Arc::clone(core),
            model_id,
            pid,
            armed: true,
        }
    }

    /// Hand ownership to whoever can now stop it.
    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SpawnedChild {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let pid = self.pid;
        let core = Arc::clone(&self.core);
        let model_id = self.model_id;

        // Drop cannot await, and the kill itself is slow — SIGTERM, a grace
        // period, then SIGKILL — so it is detached. It must still happen when
        // nobody is left to observe it, which is precisely the cancelled case.
        //
        // `try_current` because a `Drop` can run outside a runtime (a test, a
        // shutdown path); there is no reactor to spawn onto then, and a panic
        // from `Drop` would be worse than the leak it is preventing.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                match core.write().await.kill_if_pid(model_id, pid).await {
                    Ok(true) => {
                        warn!(model_id = %model_id, pid = %pid, "stopped the child of a failed launch");
                    }
                    // Already gone, or the id now names a different process —
                    // both mean this guard has nothing left to stop, and
                    // neither is worth shouting about.
                    Ok(false) => {
                        debug!(model_id = %model_id, pid = %pid, "child was already gone or replaced");
                    }
                    Err(e) => {
                        warn!(model_id = %model_id, pid = %pid, error = %e, "could not stop the child of a failed launch");
                    }
                }
            });
        } else {
            // No reactor to spawn onto — a `Drop` during runtime shutdown, or
            // in a test. `GuiProcessCore`'s own `Drop` is the backstop there;
            // it SIGKILLs everything it still tracks.
            warn!(
                model_id = %model_id,
                "no runtime to stop the child of a failed launch; \
                 GuiProcessCore::drop is the backstop"
            );
        }
    }
}

#[cfg(all(test, unix))]
#[path = "launch_tests.rs"]
mod tests;
