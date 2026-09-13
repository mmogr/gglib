//! Waiting out the daemon's own resume.
//!
//! At startup the daemon spawns `resume` just before its API starts
//! answering, and the resume can hold the serve side for fifteen seconds: it
//! starts the proxy, reserves the slot, then waits for a relay. A person who
//! typed `gglib remote enable --invite` at that daemon, most often one the
//! CLI had just started for them, met it and was refused with "already being
//! enabled", and neither call offered a code ([#1037]). Now `enable` and
//! `invite` wait for the resume and answer from what it brought up.
//!
//! The resume says it is working for its whole span, not only while it holds
//! the reservation. `turn_on` starts the proxy *before* it reserves, so a
//! command arriving in that gap would read an empty slot, go on to start the
//! proxy itself, and lose the race to reserve. A `watch` rather than a
//! `Notify`, because `wait_for` looks at the current value before it parks,
//! and so cannot miss a resume that ended a moment before it looked.
//!
//! The flag is a timing signal. What the wait found decides two things: a
//! `disable` while it waited ends the call, and a plain `enable` that waited
//! may be answered from the session the resume brought up. Everything else is
//! decided by reading the slot again, so a flag that is wrong for an instant
//! costs a wait at most.
//!
//! [#1037]: https://github.com/mmogr/gglib/issues/1037

use tokio::sync::watch;

use super::{RemoteOps, WAIT_OUT_RESUME};

/// Held for the whole of a startup resume. Dropping it is what says the
/// resume is over, so every early return, and a panic, say it too.
#[must_use = "the resume counts as over the moment this is dropped"]
pub(super) struct Resuming<'a>(&'a watch::Sender<bool>);

impl Drop for Resuming<'_> {
    fn drop(&mut self) {
        // `send_replace`, not `send`: `send` writes nothing while nobody is
        // subscribed, which is the ordinary case, and the flag would stay up.
        self.0.send_replace(false);
    }
}

/// What waiting found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Waited {
    /// No resume was working, so nothing was waited for.
    NotAtAll,
    /// A resume was working, and has ended or outlasted the wait. The slot
    /// says which, and the caller reads it.
    ForAResume,
    /// A `disable` landed while this waited.
    Cancelled,
}

impl RemoteOps {
    /// Say that the startup resume is working, until the guard is dropped.
    pub(super) fn mark_resuming(&self) -> Resuming<'_> {
        // `send_replace` for the reason the guard's `drop` gives.
        self.resuming.send_replace(true);
        Resuming(&self.resuming)
    }

    /// Wait for a startup resume that is working, if one is, for up to
    /// [`WAIT_OUT_RESUME`].
    ///
    /// A wait that runs out answers `ForAResume` all the same: the caller
    /// reads the slot next, and a resume still arming there meets the
    /// ordinary refusal, which says to wait.
    pub(super) async fn wait_out_the_resume(&self) -> Waited {
        // Subscribed to `disable` before the flag is read, so one landing
        // between the two is seen rather than missed.
        let mut disables = self.disables.subscribe();
        let mut resuming = self.resuming.subscribe();
        if !*resuming.borrow_and_update() {
            return Waited::NotAtAll;
        }
        let waited = tokio::time::timeout(WAIT_OUT_RESUME, async {
            // A `disable` and the end of the resume at the same moment is a
            // `disable`: the person said it last.
            tokio::select! {
                biased;
                _ = disables.changed() => Waited::Cancelled,
                _ = resuming.wait_for(|on| !*on) => Waited::ForAResume,
            }
        })
        .await;
        waited.unwrap_or(Waited::ForAResume)
    }
}
