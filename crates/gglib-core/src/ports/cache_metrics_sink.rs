//! Outbound port for recording prompt-cache reuse.

/// A sink for per-request prompt-cache reuse figures.
///
/// The recording site — the in-process LLM adapter's response stream — writes
/// each completed request's usage here without knowing where it lands. The
/// proxy's dashboard implements it with an in-memory
/// [`CacheMetricsStore`](crate::cache_metrics::CacheMetricsStore); a process
/// with no dashboard passes no sink at all, so recording becomes a no-op. A
/// future cross-process reporter — a `gglib chat` run posting its reuse to a
/// running proxy — would be another implementation behind this same seam,
/// needing no change to the adapter.
///
/// `cached_tokens` keeps the `Option<u32>` absent-vs-zero distinction: `None`
/// means the upstream didn't report the field, `Some(0)` means a real full
/// re-prefill. Implementations must not collapse the two.
pub trait CacheMetricsSink: Send + Sync {
    /// Record one completed request's prompt-token count and how many of those
    /// tokens the upstream served from its KV cache.
    fn record(&self, prompt_tokens: u32, cached_tokens: Option<u32>);
}
