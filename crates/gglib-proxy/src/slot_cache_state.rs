//! What the proxy remembers about the disk slot cache between requests.
//!
//! Four facts, and they move together: which session's KV state the server
//! holds in RAM, which sessions were cleared, whether everything was, and when
//! the server now running started. A clear or a restart changes several of
//! them at once, so they sit behind one lock in one value, and every change
//! is a method here. Anything that learns of a clear or a restart calls the
//! one method for it, so no caller can make half of the change.
//!
//! The rule the methods keep between them: **a session that may not be saved
//! is never hot.** A hot session skips the restore, and the restore is what
//! lets a cleared session save again, so a session that was both would never
//! be saved for as long as it stayed hot.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::warn;

/// The slot cache's state for one proxy run, shared by every request and by
/// the routes that clear the cache or load a model.
#[derive(Debug)]
pub(crate) struct SlotCacheState {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// The model and session whose KV state the server is taken to hold in
    /// RAM, when one is. A request for it skips the disk restore.
    hot: Option<(u32, String)>,
    /// Sessions cleared since their last restore. None of them is saved: a
    /// cycle that was generating when the clear ran would write back the file
    /// the clear had just deleted.
    cleared: HashSet<String>,
    /// Everything was cleared, or the server restarted, since the last
    /// restore. Nothing is saved, for the same reason.
    all_cleared: bool,
    /// Unix seconds at which the server now running started. A slot file
    /// older than this was written by an earlier server and is not restored.
    server_start_secs: u64,
}

impl Inner {
    fn may_save(&self, session_id: &str) -> bool {
        !self.all_cleared && !self.cleared.contains(session_id)
    }
}

fn unix_secs(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

impl SlotCacheState {
    /// The state of a proxy whose server started at `started`: nothing hot,
    /// nothing cleared.
    pub(crate) fn new(started: SystemTime) -> Self {
        Self {
            inner: Mutex::new(Inner {
                hot: None,
                cleared: HashSet::new(),
                all_cleared: false,
                server_start_secs: unix_secs(started),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Unix seconds at which the server now running started: the mtime
    /// guard's cutoff.
    pub(crate) fn server_start_secs(&self) -> u64 {
        self.lock().server_start_secs
    }

    /// Whether `session_id` under `model_id` is what the server holds in RAM,
    /// so its disk restore can be skipped.
    pub(crate) fn is_hot(&self, model_id: u32, session_id: &str) -> bool {
        self.lock()
            .hot
            .as_ref()
            .is_some_and(|(model, session)| *model == model_id && session == session_id)
    }

    /// Record that `session_id` under `model_id` has finished generating:
    /// the server's RAM holds its KV state now, and no other session's.
    ///
    /// A session that may not be saved was cleared, or the server restarted,
    /// while it generated. It is left cold, because its next request has to
    /// reach the restore, and the session hot before it is forgotten, because
    /// the generation replaced what the server held of it.
    pub(crate) fn mark_hot(&self, model_id: u32, session_id: &str) {
        let mut inner = self.lock();
        inner.hot = inner
            .may_save(session_id)
            .then(|| (model_id, session_id.to_owned()));
    }

    /// Whether `session_id` may be saved: not while it, or everything, has
    /// been cleared since its last restore.
    pub(crate) fn may_save(&self, session_id: &str) -> bool {
        self.lock().may_save(session_id)
    }

    /// Record a restore attempt for `session_id`, whatever came of it. The
    /// session is live again, and so is saving: left set, the clear-all flag
    /// would block every session's save for good.
    pub(crate) fn restore_attempted(&self, session_id: &str) {
        let mut inner = self.lock();
        inner.cleared.remove(session_id);
        inner.all_cleared = false;
    }

    /// Record that `session_id`'s slot files were cleared, under whichever
    /// model they were saved.
    pub(crate) fn clear_session(&self, session_id: &str) {
        let mut inner = self.lock();
        inner.cleared.insert(session_id.to_owned());
        if inner.hot.as_ref().is_some_and(|(_, hot)| hot == session_id) {
            inner.hot = None;
        }
    }

    /// Record that every slot file was cleared.
    pub(crate) fn clear_all(&self) {
        let mut inner = self.lock();
        inner.all_cleared = true;
        inner.hot = None;
    }

    /// Record that an admission reported a freshly started server at `now`.
    /// Nothing is in its RAM, and the slot files on disk are an earlier
    /// server's.
    ///
    /// One spawn can satisfy several queued requests, and each of them
    /// reports the fresh start. Only a `now` later than the recorded start,
    /// in whole seconds, counts, so the first of them records the restart
    /// and the rest find it recorded. Returns whether this call did.
    pub(crate) fn on_restart(&self, now: SystemTime) -> bool {
        let now = unix_secs(now);
        let mut inner = self.lock();
        if now <= inner.server_start_secs {
            return false;
        }
        inner.server_start_secs = now;
        inner.all_cleared = true;
        inner.hot = None;
        drop(inner);
        warn!("Llama-server restart detected — invalidating KV cache slots");
        true
    }
}

#[cfg(test)]
#[path = "slot_cache_state_tests.rs"]
mod tests;
