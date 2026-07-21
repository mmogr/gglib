//! Per-request prompt-cache telemetry.
//!
//! Records how many prompt tokens each completed request reused from
//! llama-server's KV cache instead of re-processing, sourced from
//! `usage.prompt_tokens_details.cached_tokens` (see
//! [`gglib_core::LlmStreamEvent::Usage`]). Both the streaming and
//! non-streaming forward paths report here, so neither is silently missing
//! from the totals.
//!
//! # Scope: proxied requests only
//!
//! These counters cover `/v1/chat/completions` traffic through this crate.
//! Council and other virtual-model runs are **not** included:
//! `gglib_runtime`'s `council_runner` composes its LLM adapter directly
//! against the model's `base_url`, so those calls never reach
//! [`crate::forward`].
//!
//! That separation is intentional in both directions. Routing internal agent
//! loops back through the user-facing proxy purely to collect telemetry would
//! be a U-turn for no benefit; and a council run issues many small sub-agent
//! calls whose reuse profile is nothing like a user's conversation, so
//! averaging the two would make this figure harder to read rather than more
//! complete. If council-side metrics are wanted later, the DRY route is to
//! lift this store somewhere both callers can reach — not to change how the
//! council talks to the model.
//!
//! Deliberately raw counters. Everything exposed is something the upstream
//! actually measured; nothing is derived, extrapolated, or turned into a
//! "time saved" figure. Reuse counts are exact, but what that reuse *saved*
//! depends on a counterfactual prefill that never ran — presenting an
//! estimate of it as a dashboard number would invite trust it can't earn.
//! Consumers that want a ratio can divide two figures that are both real.
//!
//! Requests whose upstream didn't report the field are counted separately
//! (`unreported_requests`) rather than folded in as zero-reuse, so a server
//! that never reports can't masquerade as a cache that never hits.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A point-in-time view of prompt-cache reuse since the proxy started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct CacheUsage {
    /// Completed requests whose upstream reported a cached-token count.
    /// The denominator for [`Self::cached_tokens`] and [`Self::prompt_tokens`].
    pub reporting_requests: u64,
    /// Completed requests whose upstream omitted the field. Excluded from
    /// every other figure here — counted so a consumer can tell "no reuse"
    /// apart from "no data".
    pub unreported_requests: u64,
    /// Total prompt tokens across [`Self::reporting_requests`].
    pub prompt_tokens: u64,
    /// Total prompt tokens served from the KV cache across those requests.
    /// Always `<= prompt_tokens`.
    pub cached_tokens: u64,
    /// Prompt tokens in the most recent reporting request.
    pub last_prompt_tokens: Option<u32>,
    /// Tokens reused from cache in the most recent reporting request.
    pub last_cached_tokens: Option<u32>,
}

/// Running totals of prompt-cache reuse.
///
/// Lock-free: every field is an independent atomic, updated with `Relaxed`
/// ordering. The counters are a display aid, not a consistency boundary — a
/// dashboard tick that reads mid-update sees one request's figures land
/// slightly out of step, which is invisible at a one-second refresh and not
/// worth a mutex on the request path.
#[derive(Debug, Default)]
pub struct CacheMetricsStore {
    reporting_requests: AtomicU64,
    unreported_requests: AtomicU64,
    prompt_tokens: AtomicU64,
    cached_tokens: AtomicU64,
    /// Last reporting request's figures, packed as `(prompt << 32) | cached`
    /// so the pair is written in one store and can never be read half-updated
    /// (e.g. a new prompt count beside the previous cached count).
    ///
    /// Validity is tracked by [`Self::has_last`] rather than by an in-band
    /// sentinel: `u64::MAX` looks unreachable but is exactly what a
    /// `(u32::MAX, u32::MAX)` request packs to, which would then read back as
    /// "nothing recorded".
    last: AtomicU64,
    /// Whether [`Self::last`] holds a real measurement. Stored with `Release`
    /// after `last`, and loaded with `Acquire` before it, so observing `true`
    /// guarantees the packed pair beside it is fully written.
    has_last: AtomicBool,
}

impl CacheMetricsStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed request's usage figures.
    ///
    /// `cached_tokens` is `None` when the upstream didn't report the field;
    /// such a request bumps only [`CacheUsage::unreported_requests`] and
    /// leaves the token totals untouched.
    pub fn record(&self, prompt_tokens: u32, cached_tokens: Option<u32>) {
        let Some(cached) = cached_tokens else {
            self.unreported_requests.fetch_add(1, Ordering::Relaxed);
            return;
        };

        // Guard the invariant rather than trusting the upstream: a cached
        // count above the prompt count would be nonsense, and letting it
        // through would make `cached_tokens > prompt_tokens` in the aggregate,
        // which reads as a cache that returned more than was asked for.
        let cached = cached.min(prompt_tokens);

        self.reporting_requests.fetch_add(1, Ordering::Relaxed);
        self.prompt_tokens
            .fetch_add(u64::from(prompt_tokens), Ordering::Relaxed);
        self.cached_tokens
            .fetch_add(u64::from(cached), Ordering::Relaxed);
        self.last.store(
            (u64::from(prompt_tokens) << 32) | u64::from(cached),
            Ordering::Relaxed,
        );
        // Released after `last` so a reader that sees `true` is guaranteed to
        // see the pair it refers to.
        self.has_last.store(true, Ordering::Release);
    }

    /// Snapshot the current totals.
    #[must_use]
    pub fn snapshot(&self) -> CacheUsage {
        let (last_prompt_tokens, last_cached_tokens) = if self.has_last.load(Ordering::Acquire) {
            let last = self.last.load(Ordering::Relaxed);
            #[allow(clippy::cast_possible_truncation)]
            (Some((last >> 32) as u32), Some(last as u32))
        } else {
            (None, None)
        };

        CacheUsage {
            reporting_requests: self.reporting_requests.load(Ordering::Relaxed),
            unreported_requests: self.unreported_requests.load(Ordering::Relaxed),
            prompt_tokens: self.prompt_tokens.load(Ordering::Relaxed),
            cached_tokens: self.cached_tokens.load(Ordering::Relaxed),
            last_prompt_tokens,
            last_cached_tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_store_reports_nothing_recorded() {
        let got = CacheMetricsStore::new().snapshot();
        assert_eq!(got, CacheUsage::default());
        assert_eq!(got.last_prompt_tokens, None);
        assert_eq!(got.last_cached_tokens, None);
    }

    #[test]
    fn records_accumulate_across_requests() {
        let store = CacheMetricsStore::new();
        store.record(10_000, Some(9_000));
        store.record(12_000, Some(11_500));

        let got = store.snapshot();
        assert_eq!(got.reporting_requests, 2);
        assert_eq!(got.prompt_tokens, 22_000);
        assert_eq!(got.cached_tokens, 20_500);
        assert_eq!(got.last_prompt_tokens, Some(12_000));
        assert_eq!(got.last_cached_tokens, Some(11_500));
    }

    /// Zero reuse is a real measurement — a full re-prefill — and must count
    /// toward the totals rather than being treated as missing data.
    #[test]
    fn zero_reuse_is_recorded_as_a_reporting_request() {
        let store = CacheMetricsStore::new();
        store.record(5_000, Some(0));

        let got = store.snapshot();
        assert_eq!(got.reporting_requests, 1);
        assert_eq!(got.prompt_tokens, 5_000);
        assert_eq!(got.cached_tokens, 0);
        assert_eq!(got.last_cached_tokens, Some(0));
    }

    /// An upstream that never reports must not look like a cache that never
    /// hits: its requests stay out of the token totals entirely.
    #[test]
    fn unreported_requests_are_counted_separately() {
        let store = CacheMetricsStore::new();
        store.record(5_000, None);
        store.record(6_000, None);

        let got = store.snapshot();
        assert_eq!(got.unreported_requests, 2);
        assert_eq!(got.reporting_requests, 0);
        assert_eq!(got.prompt_tokens, 0, "must not inflate the denominator");
        assert_eq!(got.cached_tokens, 0);
        assert_eq!(got.last_prompt_tokens, None, "no reporting request yet");
    }

    #[test]
    fn mixed_reporting_and_unreported_requests_stay_separated() {
        let store = CacheMetricsStore::new();
        store.record(1_000, Some(900));
        store.record(2_000, None);

        let got = store.snapshot();
        assert_eq!(got.reporting_requests, 1);
        assert_eq!(got.unreported_requests, 1);
        assert_eq!(got.prompt_tokens, 1_000);
        assert_eq!(got.last_prompt_tokens, Some(1_000));
    }

    /// A nonsensical upstream figure is clamped rather than propagated —
    /// otherwise the aggregate could report more tokens reused than sent.
    #[test]
    fn cached_count_is_clamped_to_the_prompt_count() {
        let store = CacheMetricsStore::new();
        store.record(100, Some(500));

        let got = store.snapshot();
        assert_eq!(got.cached_tokens, 100);
        assert_eq!(got.last_cached_tokens, Some(100));
        assert!(got.cached_tokens <= got.prompt_tokens);
    }

    /// The packed `last` pair must round-trip at the extremes, since a
    /// shift-based encoding is exactly where an off-by-32 would hide.
    #[test]
    fn last_pair_round_trips_at_u32_bounds() {
        let store = CacheMetricsStore::new();
        store.record(u32::MAX, Some(u32::MAX));

        let got = store.snapshot();
        assert_eq!(got.last_prompt_tokens, Some(u32::MAX));
        assert_eq!(got.last_cached_tokens, Some(u32::MAX));
    }

    #[test]
    fn last_reflects_only_the_most_recent_reporting_request() {
        let store = CacheMetricsStore::new();
        store.record(1_000, Some(900));
        store.record(2_000, None); // must not clobber `last`

        let got = store.snapshot();
        assert_eq!(got.last_prompt_tokens, Some(1_000));
        assert_eq!(got.last_cached_tokens, Some(900));
    }
}
