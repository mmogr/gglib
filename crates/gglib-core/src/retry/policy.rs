//! Pure backoff policy — no clock, no randomness source, no I/O.
//!
//! [`decide`] is a total function of its arguments: the caller owns the clock
//! (passing `elapsed`) and the randomness (passing `jitter_unit`), so every
//! backoff assertion in the test suite is exact and no test ever sleeps.
//! Execution lives in the adapter layers that call this.

use std::time::Duration;

/// Bounds on a retry sequence.
///
/// Two independent limits apply, and whichever trips first wins:
/// `max_attempts` caps how many times the work is tried, `total_deadline`
/// caps the wall-clock time the whole sequence may consume. The deadline is
/// what keeps a per-attempt timeout from multiplying — a 600 s send timeout
/// retried four times must not become a 40-minute hang.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts, including the first. `1` disables retrying.
    pub max_attempts: u32,
    /// Backoff base for the first retry; doubles each subsequent retry.
    pub initial_backoff: Duration,
    /// Ceiling on any single delay, including a server-supplied `Retry-After`.
    pub max_backoff: Duration,
    /// Ceiling on the wall-clock time the whole sequence may consume.
    pub total_deadline: Duration,
}

impl Default for RetryPolicy {
    /// Defaults tuned for the LLM completion path.
    ///
    /// Deliberately modest: the proxy already absorbs `ModelLoading`
    /// server-side, so a client-side retry is covering startup *contention*,
    /// which by definition means something upstream has already waited a long
    /// time. A larger budget here would stack on top of that and turn an
    /// unlucky request into a multi-minute hang.
    fn default() -> Self {
        Self {
            max_attempts: 4,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(15),
            total_deadline: Duration::from_mins(1),
        }
    }
}

/// Why a retry sequence stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveUpReason {
    /// `max_attempts` reached.
    AttemptsExhausted,
    /// `total_deadline` reached, or the next delay would overrun it.
    DeadlineExceeded,
}

impl GiveUpReason {
    /// Short, stable description for logs and observer callbacks.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AttemptsExhausted => "attempts exhausted",
            Self::DeadlineExceeded => "deadline exceeded",
        }
    }
}

/// The outcome of consulting the policy after a failed attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryDecision {
    /// Sleep for `after`, then try again.
    Retry {
        /// How long to wait before the next attempt.
        after: Duration,
    },
    /// Stop; the sequence has failed.
    GiveUp(GiveUpReason),
}

/// Decide what to do after `attempt` attempts have failed.
///
/// `attempt` counts *completed* attempts, so it is `1` after the first failure.
/// `elapsed` is the time consumed by the sequence so far. `jitter_unit` is a
/// caller-supplied value in `[0.0, 1.0)`; it is clamped defensively so a bad
/// caller cannot produce a negative or unbounded delay.
///
/// # Delay derivation
///
/// Without a server hint, this is **full jitter** — `random(0, min(cap,
/// base·2ⁿ))`. The failure mode being defended against is several clients
/// colliding on one model's startup, and full jitter is the variant that
/// decorrelates them most aggressively. Fixed backoff would have every waiter
/// wake together and collide again.
///
/// With a `Retry-After`, the server's value is treated as a **floor** rather
/// than replaced by jitter: retrying earlier than the server asked just burns
/// an attempt against a resource known to be unready. A small jitter of up to
/// `initial_backoff` is added on top, so concurrent clients handed the same
/// `Retry-After` still spread out. The floor is clamped to `max_backoff` first,
/// so a buggy or hostile upstream cannot park a request indefinitely.
///
/// A delay that would overrun `total_deadline` yields
/// [`GiveUpReason::DeadlineExceeded`] rather than a truncated sleep: waking
/// early, before the moment the server nominated, is worse than stopping.
#[must_use]
pub fn decide(
    policy: &RetryPolicy,
    attempt: u32,
    server_retry_after: Option<Duration>,
    elapsed: Duration,
    jitter_unit: f64,
) -> RetryDecision {
    if attempt >= policy.max_attempts {
        return RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted);
    }
    if elapsed >= policy.total_deadline {
        return RetryDecision::GiveUp(GiveUpReason::DeadlineExceeded);
    }

    let delay = server_retry_after.map_or_else(
        || full_jitter(policy, attempt, jitter_unit),
        |hint| honour_server_hint(policy, hint, jitter_unit),
    );

    if elapsed.saturating_add(delay) >= policy.total_deadline {
        return RetryDecision::GiveUp(GiveUpReason::DeadlineExceeded);
    }

    RetryDecision::Retry { after: delay }
}

/// Server hint as a floor, clamped to `max_backoff`, plus decorrelating jitter.
fn honour_server_hint(policy: &RetryPolicy, hint: Duration, jitter_unit: f64) -> Duration {
    let floor = hint.min(policy.max_backoff);
    let spread = scale(policy.initial_backoff, jitter_unit);
    floor.saturating_add(spread)
}

/// Full jitter over an exponentially growing, capped window.
fn full_jitter(policy: &RetryPolicy, attempt: u32, jitter_unit: f64) -> Duration {
    // `attempt` is 1-based, so the first retry uses `initial_backoff` unscaled.
    // Shift width is capped well below `u64::BITS` so the doubling cannot
    // overflow regardless of how large `max_attempts` is configured.
    let exponent = attempt.saturating_sub(1).min(32);
    let factor = 1u64 << exponent;

    let base_ms = duration_ms(policy.initial_backoff).saturating_mul(factor);
    let window_ms = base_ms.min(duration_ms(policy.max_backoff));

    scale(Duration::from_millis(window_ms), jitter_unit)
}

/// Multiply a duration by a unit fraction, clamping the fraction to `[0.0, 1.0]`.
fn scale(window: Duration, jitter_unit: f64) -> Duration {
    let unit = if jitter_unit.is_finite() {
        jitter_unit.clamp(0.0, 1.0)
    } else {
        0.0
    };
    // `as u64` on a non-negative, finite f64 bounded by `window_ms` cannot
    // wrap; the product is at most the window itself.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    #[allow(clippy::cast_sign_loss)]
    let scaled_ms = (duration_ms(window) as f64 * unit) as u64;
    Duration::from_millis(scaled_ms)
}

/// Saturating millisecond view of a duration.
fn duration_ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}
