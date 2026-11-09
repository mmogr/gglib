//! Prompt-cache reuse telemetry.
//!
//! Records how many prompt tokens each completed request reused from
//! llama-server's KV cache instead of re-processing, sourced from
//! `usage.prompt_tokens_details.cached_tokens` (see
//! [`crate::LlmStreamEvent::Usage`]).
//!
//! The store is location-neutral — a lock-free counter with a
//! `serde::Serialize` snapshot and no dependency on any particular request
//! path — so more than one caller can keep its own instance. The proxy keeps
//! one for the traffic it forwards; a separate instance covers the in-process
//! agent path (GUI and CLI chat). The two populations are reported side by
//! side, never merged: an agent turn's many small tool-driven calls have a reuse
//! profile nothing like a user's conversation, so averaging them would make
//! either figure harder to read.
//!
//! Raw counters, and one ratio of two of them. Every count is something the
//! upstream actually measured; nothing is extrapolated or turned into a
//! "time saved" figure. Reuse counts are exact, but what that reuse *saved*
//! depends on a counterfactual prefill that never ran — presenting an estimate
//! of it as a dashboard number would invite trust it can't earn. The share
//! of prompt tokens served from the cache is two real figures divided, made
//! here once ([`CacheUsage::reuse_percent`]) and said here once
//! ([`CacheUsage::summary`]), so every surface shows the same percent.
//!
//! Requests whose upstream didn't report the field are counted separately
//! (`unreported_requests`) rather than folded in as zero-reuse, so a server
//! that never reports can't masquerade as a cache that never hits.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A point-in-time view of prompt-cache reuse since the proxy started.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct CacheUsage {
    /// Completed requests whose upstream reported a cached-token count.
    /// The denominator for [`Self::cached_tokens`] and [`Self::prompt_tokens`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub reporting_requests: u64,
    /// Completed requests whose upstream omitted the field. Excluded from
    /// every other figure here — counted so a consumer can tell "no reuse"
    /// apart from "no data".
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub unreported_requests: u64,
    /// Total prompt tokens across [`Self::reporting_requests`].
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub prompt_tokens: u64,
    /// Total prompt tokens served from the KV cache across those requests.
    /// Always `<= prompt_tokens`.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub cached_tokens: u64,
    /// Prompt tokens in the most recent reporting request.
    pub last_prompt_tokens: Option<u32>,
    /// Tokens reused from cache in the most recent reporting request.
    pub last_cached_tokens: Option<u32>,
    /// [`Self::cached_tokens`] as a whole percent of [`Self::prompt_tokens`],
    /// a half rounded up; `None` before any prompt token was measured.
    pub reuse_percent: Option<u8>,
    /// The reuse as every surface says it, "Prompt cache 71% reused"; `None`
    /// with [`Self::reuse_percent`].
    pub summary: Option<String>,
    /// [`Self::last_cached_tokens`] as a whole percent of
    /// [`Self::last_prompt_tokens`], a half rounded up; `None` before a
    /// request reported, and for one with an empty prompt.
    pub last_reuse_percent: Option<u8>,
}

impl CacheUsage {
    /// `cached` as a whole percent of `prompt`, a half rounded up; `None`
    /// when no prompt token was measured. A cached count above the prompt
    /// count says 100, as the store already clamps it.
    #[must_use]
    pub fn reuse_percent(prompt: u64, cached: u64) -> Option<u8> {
        if prompt == 0 {
            return None;
        }
        let cached = u128::from(cached.min(prompt));
        let prompt = u128::from(prompt);
        let percent = (cached * 200 + prompt) / (prompt * 2);
        u8::try_from(percent).ok()
    }

    /// The reuse in words: "Prompt cache 71% reused".
    #[must_use]
    pub fn summary(percent: u8) -> String {
        format!("Prompt cache {percent}% reused")
    }
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

        let prompt_tokens = self.prompt_tokens.load(Ordering::Relaxed);
        let cached_tokens = self.cached_tokens.load(Ordering::Relaxed);
        let reuse_percent = CacheUsage::reuse_percent(prompt_tokens, cached_tokens);
        CacheUsage {
            reporting_requests: self.reporting_requests.load(Ordering::Relaxed),
            unreported_requests: self.unreported_requests.load(Ordering::Relaxed),
            prompt_tokens,
            cached_tokens,
            last_prompt_tokens,
            last_cached_tokens,
            reuse_percent,
            summary: reuse_percent.map(CacheUsage::summary),
            last_reuse_percent: last_prompt_tokens.zip(last_cached_tokens).and_then(
                |(prompt, cached)| CacheUsage::reuse_percent(u64::from(prompt), u64::from(cached)),
            ),
        }
    }
}

/// The in-process store is the dashboard's [`UsageSink`]: the adapter records
/// straight into the atomics the dashboard snapshots.
///
/// `completion_tokens` is deliberately ignored — this store exists to answer
/// "how much of the prompt was served from cache", and the dashboard has no
/// generation-side figure to render.
///
/// [`UsageSink`]: crate::ports::UsageSink
impl crate::ports::UsageSink for CacheMetricsStore {
    fn record(&self, prompt_tokens: u32, _completion_tokens: u32, cached_tokens: Option<u32>) {
        Self::record(self, prompt_tokens, cached_tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The share and its words, from known counts: none before a prompt
    /// token, a half rounded up, a full re-prefill a real zero.
    #[test]
    fn the_reuse_is_a_whole_percent_of_the_prompt_said_in_words() {
        let store = CacheMetricsStore::new();
        assert_eq!(store.snapshot().reuse_percent, None);
        assert_eq!(store.snapshot().summary, None);

        store.record(10_000, Some(7_050));
        let got = store.snapshot();
        assert_eq!(got.reuse_percent, Some(71));
        assert_eq!(got.summary.as_deref(), Some("Prompt cache 71% reused"));
        assert_eq!(got.last_reuse_percent, Some(71));

        // The total and the last request apart: 7,950 of 12,000 is 66%,
        // the last 900 of 2,000 is 45%.
        store.record(2_000, Some(900));
        let got = store.snapshot();
        assert_eq!(got.reuse_percent, Some(66));
        assert_eq!(got.last_reuse_percent, Some(45));

        assert_eq!(CacheUsage::reuse_percent(5_000, 0), Some(0));
        assert_eq!(CacheUsage::reuse_percent(200, 1), Some(1));
        assert_eq!(CacheUsage::reuse_percent(200, 0), Some(0));
        assert_eq!(CacheUsage::reuse_percent(3, 1), Some(33));
        assert_eq!(CacheUsage::reuse_percent(100, 100), Some(100));
        assert_eq!(CacheUsage::reuse_percent(100, 250), Some(100));
        assert_eq!(CacheUsage::reuse_percent(0, 0), None);
    }

    /// A request whose upstream reported no cache count gives no percent:
    /// "no data" is not "no reuse".
    #[test]
    fn an_upstream_that_never_reports_gives_no_percent() {
        let store = CacheMetricsStore::new();
        store.record(5_000, None);
        assert_eq!(store.snapshot().reuse_percent, None);
        assert_eq!(store.snapshot().summary, None);
        assert_eq!(store.snapshot().last_reuse_percent, None);
    }

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
