//! Outbound port for recording per-request token usage.

/// A sink for one completed request's token usage.
///
/// The recording site — the in-process LLM adapter's response stream — writes
/// each completed request's usage here without knowing where it lands. The
/// proxy's dashboard implements it with an in-memory
/// [`CacheMetricsStore`](crate::cache_metrics::CacheMetricsStore), which keeps
/// only the prompt-cache figures; the benchmark harness implements it with a
/// completion-token accumulator, so a task whose agent loop aborts still
/// reports the tokens it burned. A process that wants neither passes no sink at
/// all, so recording becomes a no-op. A future cross-process reporter — a
/// `gglib chat` run posting its usage to a running proxy — would be another
/// implementation behind this same seam, needing no change to the adapter.
///
/// A request whose upstream reported no usage at all records nothing — the
/// recording site simply does not call this — so "never called" is how absence
/// is expressed, and every reported figure is a real measurement.
///
/// `cached_tokens` keeps the `Option<u32>` absent-vs-zero distinction: `None`
/// means the upstream didn't report the field, `Some(0)` means a real full
/// re-prefill. Implementations must not collapse the two.
pub trait UsageSink: Send + Sync {
    /// Record one completed request's token usage: the prompt-token count, how
    /// many tokens the model generated, and how many of the prompt tokens the
    /// upstream served from its KV cache.
    fn record(&self, prompt_tokens: u32, completion_tokens: u32, cached_tokens: Option<u32>);
}
