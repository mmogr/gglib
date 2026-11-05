//! The daemon's announcement of every load and unload, whoever caused it,
//! and the health watch that runs for as long as a model is loaded.
//!
//! [`ResidencyEvents`] is the daemon's [`ResidencyObserver`]. The resident
//! set tells it of each model that became resident and each that stopped
//! being, and it sends `server_started` and `server_stopped` on
//! `/api/events` and watches the server's health in between. A start the
//! GUI asked for, a swap the proxy made for a request, a recycle, a model
//! `gglib serve` loaded and a render's retirement all reach the clients the
//! same way; `ServerOps` announces nothing itself.

use std::collections::HashMap;
use std::fmt;
use std::pin::pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, ProcessHandle, ResidencyObserver, ServerHealthStatus};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// How often a loaded model's server is asked how it is.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Sends a frame for every load and unload, and watches each loaded
/// server's health until its unload.
pub(crate) struct ResidencyEvents {
    emitter: Arc<dyn AppEventEmitter>,
    /// The watch on each loaded model, by model id.
    watched: Mutex<HashMap<u32, Watch>>,
    /// Numbers each watch: `ServerHealthChanged.server_id`.
    next_server_id: AtomicI64,
}

/// One loaded model: the name it was announced with, and the token that
/// ends its health watch.
struct Watch {
    model_name: String,
    cancel: CancellationToken,
}

impl ResidencyEvents {
    pub(crate) fn new(emitter: Arc<dyn AppEventEmitter>) -> Self {
        Self {
            emitter,
            watched: Mutex::new(HashMap::new()),
            next_server_id: AtomicI64::new(1),
        }
    }

    /// The ids of the models being watched, in order.
    #[cfg(test)]
    pub(crate) fn watched(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.lock().keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// The watches, recovering from a poisoned lock: a panic in one short
    /// section is no reason to stop announcing.
    fn lock(&self) -> MutexGuard<'_, HashMap<u32, Watch>> {
        self.watched
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl fmt::Debug for ResidencyEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResidencyEvents")
            .field("watched", &self.lock().len())
            .finish_non_exhaustive()
    }
}

impl ResidencyObserver for ResidencyEvents {
    fn loaded(&self, server: ProcessHandle) {
        let Ok(model_id) = u32::try_from(server.model_id) else {
            warn!(
                model_id = server.model_id,
                "a loaded model has no library id"
            );
            return;
        };
        let server_id = self.next_server_id.fetch_add(1, Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let previous = self.lock().insert(
            model_id,
            Watch {
                model_name: server.model_name.clone(),
                cancel: cancel.clone(),
            },
        );
        if let Some(previous) = previous {
            // Loaded again with no unload between: one watch per model, the
            // newer one.
            previous.cancel.cancel();
            warn!(model_id, "a model was announced loaded twice");
        }
        self.emitter.emit(AppEvent::server_started(
            server.model_id,
            &server.model_name,
            server.port,
        ));
        tokio::spawn(watch(server, server_id, cancel, Arc::clone(&self.emitter)));
    }

    fn unloaded(&self, model_id: u32) {
        let Some(watch) = self.lock().remove(&model_id) else {
            warn!(
                model_id,
                "a model was announced unloaded that was never announced loaded"
            );
            return;
        };
        // Ended before the stop that follows, so the stop is not reported as
        // the server dying.
        watch.cancel.cancel();
        self.emitter.emit(AppEvent::server_stopped(
            i64::from(model_id),
            watch.model_name,
        ));
    }
}

/// Report each change in `server`'s health until `cancel` ends the watch.
async fn watch(
    server: ProcessHandle,
    server_id: i64,
    cancel: CancellationToken,
    emitter: Arc<dyn AppEventEmitter>,
) {
    let (model_id, port) = (server.model_id, server.port);
    // Asked the way its runtime answers: an `sd-server` has no `/health`,
    // and is asked `/v1/models` instead.
    let monitor =
        gglib_runtime::ServerHealthMonitor::new(server, HEALTH_CHECK_INTERVAL, cancel.clone());
    let mut stream = pin!(monitor.monitor());
    while let Some(status) = stream.next().await {
        // A check that was in flight when the model was unloaded says
        // nothing after `server_stopped`.
        if cancel.is_cancelled() {
            break;
        }
        let detail = match &status {
            ServerHealthStatus::Degraded { reason } => Some(reason.clone()),
            ServerHealthStatus::Unreachable { last_error } => Some(last_error.clone()),
            ServerHealthStatus::Healthy | ServerHealthStatus::ProcessDied => None,
        };
        debug!(server_id, model_id, port, ?status, "health changed");
        emitter.emit(AppEvent::ServerHealthChanged {
            server_id,
            model_id,
            status,
            detail,
            timestamp: now_ms(),
        });
    }
    debug!(server_id, model_id, "health watch ended");
}

/// Milliseconds since the Unix epoch, as a health frame is stamped.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(u64::MAX, |d| {
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
#[path = "residency_events_tests.rs"]
mod tests;
