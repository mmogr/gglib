//! Outbound port for reporting retry activity.

use std::time::Duration;

/// A sink for retry activity on a request that is being re-attempted.
///
/// The reporting site — the LLM completion adapter's retry loop — records that
/// it is backing off without knowing where the notice lands. The agent HTTP
/// handler implements it by pushing an
/// [`AgentEvent::SystemWarning`](crate::domain::agent::AgentEvent::SystemWarning)
/// into the SSE stream it already owns, so a waiting user sees "retrying"
/// rather than a frozen cursor; a one-shot CLI path with nothing to notify
/// passes no observer at all, making the calls no-ops.
///
/// This mirrors [`CacheMetricsSink`](crate::ports::CacheMetricsSink) — the same
/// optional-upward-reporting seam, so both are wired into the adapter the same
/// way and neither couples it to a transport.
///
/// Implementations are called from inside the request path and must not block:
/// a slow observer delays the retry it is describing.
pub trait RetryObserver: Send + Sync {
    /// A retry has been scheduled. `attempt` counts completed attempts, so it
    /// is `1` on the first retry. `delay` is how long the caller is about to
    /// wait, and `reason` describes the failure being retried.
    fn on_retry(&self, attempt: u32, delay: Duration, reason: &str);

    /// The sequence gave up. `reason` describes which limit was reached — see
    /// [`GiveUpReason::as_str`](crate::retry::GiveUpReason::as_str).
    fn on_exhausted(&self, attempts: u32, elapsed: Duration, reason: &str);
}
