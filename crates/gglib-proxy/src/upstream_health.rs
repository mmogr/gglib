//! Consecutive-failure watchdog for the upstream llama-server.
//!
//! The `/health` fast-path check in `gglib_runtime`'s process manager catches
//! a llama-server that has *crashed or wedged hard* (its `/health` endpoint
//! stops returning `200`). It does **not** catch the subtler failure modes this
//! module targets: a server whose `/health` is still green but which has
//! degraded to the point of producing **empty responses**, **dying
//! mid-generation**, or never returning the first token. Each manifests to the
//! client as a turn that simply failed.
//!
//! [`UpstreamHealth`] accumulates such degraded outcomes across requests. When
//! [`STRIKE_THRESHOLD`] consecutive strikes occur it raises a one-shot
//! "recycle requested" flag; the chat handler consumes that flag before the
//! next request and proactively stops the current model, forcing a fresh
//! respawn — the same cure a human applies by restarting the proxy, automated.
//!
//! ## What may cast a vote
//!
//! Every recorded outcome is a claim about the *upstream*. Outcomes the client
//! caused — chiefly hanging up mid-turn — are counted for observability and
//! then abstain, because they are evidence about a person, not a server. See
//! [`StreamVerdict`], whose variants exist precisely to keep those two apart.
//!
//! ## Concurrency design
//!
//! Lock-free atomics only. Every operation is a handful of `Relaxed` atomic
//! reads/writes with no `.await`, so it is cheap on the hot path and cannot
//! hold anything across an await point.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Number of *consecutive* degraded responses (empty streams or first-byte
/// timeouts) that trip a proactive recycle of the upstream model server.
///
/// Two is deliberately low: a single empty response can be a legitimate model
/// artefact, but two in a row is a strong signal the upstream has degraded and
/// is worth the cost of a recycle.
pub(crate) const STRIKE_THRESHOLD: u32 = 2;

/// What one streamed turn revealed about the upstream's health.
///
/// Deliberately four states rather than a bool. The two that a bool collapses
/// are the ones that made the watchdog read the world backwards:
///
/// * [`Self::UpstreamError`] — the turn *died upstream*. Under a bool this
///   arrived as "healthy", because the error frame is renderable and the drain
///   loop marks renderable frames as visible output. A server dying mid-stream
///   on every single request therefore reset the streak every time and the
///   recycle watchdog could never fire — the failure most deserving of a
///   recycle was the one that guaranteed none.
/// * [`Self::ClientAborted`] — the person hung up. Under a bool this arrived as
///   "empty", because no visible output was produced. Nothing was learned about
///   the upstream, but a strike was recorded anyway; with
///   [`STRIKE_THRESHOLD`] at two, cancelling two generations in a row was
///   enough to recycle a perfectly healthy model server.
///
/// A verdict is about the *upstream*, never about the client. When the client's
/// behaviour is what ended the turn, the right answer is to abstain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamVerdict {
    /// Client-renderable model output reached the client. The upstream is
    /// producing; the streak resets.
    Healthy,
    /// The turn produced nothing a client can render — no content, no tool
    /// call, not even recovered normalization text. The original degradation
    /// this watchdog was built for.
    Empty,
    /// The turn died upstream mid-generation: the model server emitted an
    /// error event, or the byte stream itself broke. Strictly stronger
    /// evidence of upstream sickness than an empty turn.
    UpstreamError,
    /// The client disconnected before the turn produced anything. Says nothing
    /// about the upstream, so it is recorded as nothing at all — neither a
    /// strike nor a reset.
    ClientAborted,
}

/// Serializable, point-in-time view of the watchdog's cumulative counters.
///
/// Surfaced in the proxy dashboard so the degradation this crate guards
/// against is diagnosable at a glance instead of only in logs.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct UpstreamHealthSnapshot {
    /// Current consecutive-strike streak (resets on any healthy response).
    pub consecutive_strikes: u32,
    /// Total empty responses observed since the proxy started.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_empty_responses: u64,
    /// Total turns that died upstream mid-generation since the proxy started.
    ///
    /// Separate from [`Self::total_empty_responses`] because the two are
    /// different illnesses: an empty turn is a model that produced nothing, a
    /// stream error is a model server that fell over. Both strike, but folding
    /// them together would report a crashing server as a quiet one.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_upstream_errors: u64,
    /// Total first-byte deadline expiries since the proxy started.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_first_byte_timeouts: u64,
    /// Total client disconnects that ended a turn before any output, since the
    /// proxy started.
    ///
    /// Recorded for observability only — these deliberately do not strike. A
    /// rising count next to a flat strike count is the signal that people are
    /// cancelling, not that the upstream is sick.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_client_aborts: u64,
    /// Total proactive recycles triggered since the proxy started.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_recycles: u64,
    /// Of those, how many could not actually be carried out because stopping
    /// the model server failed.
    ///
    /// A non-zero value here means the watchdog is firing and being ignored —
    /// a different problem from a healthy upstream, and one that is otherwise
    /// invisible because the request that triggered it proceeds regardless.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_recycle_failures: u64,
}

/// Lock-free tracker of consecutive degraded upstream responses.
///
/// Wrap in `Arc` and share across handler tasks.
#[derive(Debug, Default)]
pub struct UpstreamHealth {
    /// Count of consecutive degraded outcomes since the last healthy one.
    consecutive_strikes: AtomicU32,
    /// One-shot flag: set when the strike threshold is reached, cleared by
    /// [`UpstreamHealth::take_recycle_request`].
    recycle_requested: AtomicBool,
    /// Cumulative counters for observability (never reset).
    total_empty_responses: AtomicU64,
    total_upstream_errors: AtomicU64,
    total_first_byte_timeouts: AtomicU64,
    total_client_aborts: AtomicU64,
    total_recycles: AtomicU64,
    total_recycle_failures: AtomicU64,
}

impl UpstreamHealth {
    /// Create a tracker with a zeroed strike counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the terminal outcome of a streamed response.
    ///
    /// Only [`StreamVerdict::Healthy`] resets the streak, and only a verdict
    /// that actually indicts the upstream strikes it. See [`StreamVerdict`] for
    /// why the two non-obvious states exist.
    ///
    /// "Renderable model output" is load-bearing for `Healthy`: a turn whose
    /// entire output arrived as `reasoning_content` renders as an empty
    /// response in clients that collapse reasoning, so counting it healthy
    /// resets this streak on every retry and the recycle threshold is never
    /// reached. Callers derive the verdict from
    /// [`StreamOutcome::health_verdict`], which encodes that precedence once.
    ///
    /// [`StreamOutcome::health_verdict`]: crate::forward::StreamOutcome
    pub fn record_stream_outcome(&self, verdict: StreamVerdict) {
        match verdict {
            StreamVerdict::Healthy => {
                self.consecutive_strikes.store(0, Ordering::Relaxed);
            }
            StreamVerdict::Empty => {
                self.total_empty_responses.fetch_add(1, Ordering::Relaxed);
                self.record_strike();
            }
            StreamVerdict::UpstreamError => {
                self.total_upstream_errors.fetch_add(1, Ordering::Relaxed);
                self.record_strike();
            }
            // Abstain: the client ended the turn, so it carries no evidence
            // either way. Counted for observability, never scored.
            StreamVerdict::ClientAborted => {
                self.total_client_aborts.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Record a first-byte deadline expiry — always a strike, since the
    /// upstream failed to begin responding at all.
    pub fn record_timeout(&self) {
        self.total_first_byte_timeouts
            .fetch_add(1, Ordering::Relaxed);
        self.record_strike();
    }

    fn record_strike(&self) {
        let strikes = self.consecutive_strikes.fetch_add(1, Ordering::Relaxed) + 1;
        if strikes >= STRIKE_THRESHOLD {
            self.recycle_requested.store(true, Ordering::Relaxed);
        }
    }

    /// Consume the recycle request, if any.
    ///
    /// Returns `true` at most once per tripped threshold. On a `true` return
    /// the strike counter is also reset, so the freshly recycled server starts
    /// with a clean slate.
    pub fn take_recycle_request(&self) -> bool {
        let requested = self.recycle_requested.swap(false, Ordering::Relaxed);
        if requested {
            self.consecutive_strikes.store(0, Ordering::Relaxed);
            self.total_recycles.fetch_add(1, Ordering::Relaxed);
        }
        requested
    }

    /// Put back a recycle request that was consumed but could not be carried
    /// out.
    ///
    /// [`Self::take_recycle_request`] clears the flag *and* zeroes the streak,
    /// on the assumption that the caller is about to act on it. When the stop
    /// then fails, the upstream is still degraded but the evidence for that has
    /// just been discarded — so without this the watchdog has to accumulate
    /// [`STRIKE_THRESHOLD`] fresh strikes before it will try again, against a
    /// server already known to be sick.
    ///
    /// Deliberately does not touch `total_recycles`: that counts recycles
    /// *triggered*, which this one was. The failure is counted separately.
    pub fn rearm_recycle(&self) {
        self.total_recycle_failures.fetch_add(1, Ordering::Relaxed);
        self.recycle_requested.store(true, Ordering::Relaxed);
    }

    /// Serializable snapshot of the cumulative counters for the dashboard.
    #[must_use]
    pub fn snapshot(&self) -> UpstreamHealthSnapshot {
        UpstreamHealthSnapshot {
            consecutive_strikes: self.consecutive_strikes.load(Ordering::Relaxed),
            total_empty_responses: self.total_empty_responses.load(Ordering::Relaxed),
            total_upstream_errors: self.total_upstream_errors.load(Ordering::Relaxed),
            total_first_byte_timeouts: self.total_first_byte_timeouts.load(Ordering::Relaxed),
            total_client_aborts: self.total_client_aborts.load(Ordering::Relaxed),
            total_recycles: self.total_recycles.load(Ordering::Relaxed),
            total_recycle_failures: self.total_recycle_failures.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
#[path = "upstream_health_tests.rs"]
mod tests;
