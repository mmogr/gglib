//! Outbound ports for the loop guard's log: a sink and a reader.
//!
//! The proxy writes to the sink on the request path; the daemon, the CLI and
//! the GUI read through the reader.
//!
//! Two traits because the two sides have nothing in common but the data. The
//! sink is synchronous and must never block or fail a request — the proxy
//! holds it as an `Option`, and no sink at all makes recording a no-op, the
//! [`UsageSink`](super::UsageSink) shape. The reader is asynchronous and only
//! ever runs off the request path. `gglib-db` implements both; nothing on the
//! proxy's side of the boundary knows it is `SQLite`.
//!
//! What the log holds and why is [`crate::domain::loop_guard_log`].

use async_trait::async_trait;

use super::RepositoryError;
use crate::domain::loop_guard_log::{LoopGuardTripDay, LoopGuardTripEvent};
use crate::settings::LoopGuardMode;

/// Where the loop guard's step records what it did.
///
/// Both methods run on the request path, so an implementation must return at
/// once: queue, count and move on. A write it cannot make is counted and
/// dropped, never waited for, and never turned into an error for the request
/// being guarded; the count reaches a person only as a warning in the log of
/// the process that holds the sink.
pub trait LoopGuardTripSink: Send + Sync {
    /// Record one decision the guard took.
    fn record_trip(&self, event: LoopGuardTripEvent);

    /// Count one request the guard scanned under `mode`, on the day `at_secs`
    /// falls on. Called for every scanned request, trip or not — it is the
    /// denominator every trip is read against.
    fn record_scan(&self, model_name: &str, mode: LoopGuardMode, at_secs: u64);
}

/// Reads the loop guard's log back.
#[async_trait]
pub trait LoopGuardTripLog: Send + Sync {
    /// Every day from `first_day` on (an [`epoch_day`]) that either table
    /// holds a row for, one entry per model, gglib version and mode, newest
    /// day first. A day with scans and no trips is included, with `trips` at
    /// zero; so is a trip whose scan was lost, with `scanned` at zero.
    ///
    /// [`epoch_day`]: crate::domain::loop_guard_log::epoch_day
    async fn summary(&self, first_day: i64) -> Result<Vec<LoopGuardTripDay>, RepositoryError>;
}
