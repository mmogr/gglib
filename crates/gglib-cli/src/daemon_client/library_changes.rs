//! What a command changes in the library, told to the daemon that serves it.
//!
//! `ModelOps` emits an event for each change it stores, and a daemon sends
//! those to every client attached to it. A `gglib model …` command runs the
//! same `ModelOps` in a process of its own, which no client is attached to,
//! so an app open on the daemon would go on showing the row as it was. The
//! command keeps what its `ModelOps` emitted and, once it is over, posts
//! each event to the daemon, which sends it on its stream as it sends its
//! own.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use anyhow::Result;
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;

use super::{DaemonHandle, auth, paths, running};
use crate::bootstrap::CliContext;

/// The events a command's `ModelOps` emitted, kept until the daemon is told
/// them.
#[derive(Default)]
pub(crate) struct LibraryChanges(Mutex<Vec<AppEvent>>);

impl LibraryChanges {
    /// Post every event emitted since the last call to the daemon that
    /// serves this library, when it is running. A command that changed
    /// nothing asks nothing of anybody.
    ///
    /// That daemon is the one whose token is under this data root, where it
    /// mints one at every start. With no token to read there is no such
    /// daemon to speak to, and whatever holds the daemon's port is asked
    /// nothing: the key [`running`] would fall back to says nothing of which
    /// library a daemon serves. With one, it is the credential `running`
    /// presents, and a daemon on another data root refuses it.
    ///
    /// The change is stored whether or not anybody is told of it, so nothing
    /// here fails the command or is reported: with no daemon, or one that
    /// does not take an event, an open app is as stale as it was before a
    /// daemon could be told, until it is refreshed. What the probe says of
    /// the daemon it found, a build or a switch that differs, it says here
    /// as it does for any command.
    pub(crate) async fn tell_daemon(&self, ctx: &CliContext) {
        let made = std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner));
        if made.is_empty() || auth::daemon_token().is_none() {
            return;
        }
        let Ok(daemon) = running(ctx).await else {
            return;
        };
        for event in &made {
            if let Err(e) = daemon.send_event(event).await {
                tracing::debug!("the daemon was not told of a library change: {e:#}");
                return;
            }
        }
    }
}

impl AppEventEmitter for LibraryChanges {
    fn emit(&self, event: AppEvent) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

impl DaemonHandle {
    /// Have the daemon send `event` on its event stream.
    async fn send_event(&self, event: &AppEvent) -> Result<()> {
        let response = self
            .post(paths::EVENTS_PATH)
            .json(event)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Self::expect_ok(response).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "library_changes_tests.rs"]
pub(crate) mod tests;
