//! Abort-surviving completion-token tally for one benchmark task.
//!
//! The agent loop sums completion tokens in a local and moves them into its
//! `AgentRunOutput` — so a run the loop or stagnation guard aborts returns an
//! `Err` and the count is lost. That is exactly the run whose token cost is
//! most worth knowing: an arm that burns tokens until a guard stops it looks
//! *free* if its tokens are dropped, and the throughput figure computed from
//! the survivors is measured on the tasks that behaved.
//!
//! [`TaskUsageTally`] closes that hole the same way
//! [`ScoringToolExecutorPort::call_log_handle`](super::ScoringToolExecutorPort::call_log_handle)
//! closes it for tool calls: the harness keeps a handle, hands a clone to the
//! adapter, and reads it afterwards regardless of how the run ended.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use gglib_core::ports::UsageSink;

/// Accumulates one task's completion tokens across every turn of its run.
///
/// Held by the harness *and* by the adapter, so the total is readable after the
/// run whether it completed or a guard aborted it.
///
/// Retries do not double-count: the adapter retries a transient failure before
/// any response body byte is read, so a failed attempt produces no stream and
/// therefore no usage frame.
#[derive(Debug, Default)]
pub(crate) struct TaskUsageTally {
    completion_tokens: AtomicU64,
    reported: AtomicBool,
}

impl TaskUsageTally {
    /// A tally with nothing recorded yet.
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Total completion tokens across every turn whose upstream reported usage.
    ///
    /// `None` when no turn reported any — an upstream that omits `usage`
    /// entirely — which stays distinct from a measured zero.
    pub(crate) fn total_completion_tokens(&self) -> Option<u64> {
        self.reported
            .load(Ordering::Relaxed)
            .then(|| self.completion_tokens.load(Ordering::Relaxed))
    }
}

impl UsageSink for TaskUsageTally {
    fn record(&self, _prompt_tokens: u32, completion_tokens: u32, _cached_tokens: Option<u32>) {
        self.completion_tokens
            .fetch_add(u64::from(completion_tokens), Ordering::Relaxed);
        self.reported.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upstream that reports no usage must stay distinguishable from one
    /// that generated nothing.
    #[test]
    fn an_unreported_run_leaves_the_tally_unmeasured() {
        let tally = TaskUsageTally::new();
        assert_eq!(tally.total_completion_tokens(), None);
    }

    #[test]
    fn a_measured_zero_is_not_absence() {
        let tally = TaskUsageTally::new();
        tally.record(100, 0, None);
        assert_eq!(tally.total_completion_tokens(), Some(0));
    }

    /// The agent loop calls the adapter once per turn, so the tally must sum
    /// rather than overwrite — this is what a guard-aborted multi-turn run
    /// reports.
    #[test]
    fn tokens_accumulate_across_turns() {
        let tally = TaskUsageTally::new();
        tally.record(100, 32_550, Some(0));
        tally.record(200, 32_538, Some(64));
        tally.record(300, 12, None);
        assert_eq!(tally.total_completion_tokens(), Some(65_100));
    }
}
