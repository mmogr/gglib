//! Outbound port for the loop guard's log: the sink the proxy writes to.
//!
//! The sink is synchronous and must never block or fail a request — the proxy
//! holds it as an `Option`, and no sink at all makes recording a no-op, the
//! [`UsageSink`](super::UsageSink) shape. `gglib-db` implements it; nothing on
//! the proxy's side of the boundary knows it is `SQLite`.
//!
//! Reading the log back has no port. Only the daemon and the CLI read it,
//! off the request path, and both already hold the database: they call
//! `gglib-db`'s `SqliteLoopGuardTripLog` directly.
//!
//! What the log holds and why is [`crate::domain::loop_guard_log`].

use crate::domain::loop_guard_log::LoopGuardTripEvent;
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
