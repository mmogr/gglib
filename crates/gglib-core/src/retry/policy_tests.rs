//! Unit tests for the pure backoff policy.
//!
//! Every assertion is exact: `decide` takes the clock and the randomness as
//! arguments, so there is nothing to stub and no test sleeps.

use std::time::Duration;

use super::*;

/// Policy with round numbers, so expected delays are readable by inspection.
fn policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 4,
        initial_backoff: Duration::from_secs(1),
        max_backoff: Duration::from_secs(15),
        total_deadline: Duration::from_mins(1),
    }
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Unwrap a `Retry`, or fail with the decision that was actually returned.
#[track_caller]
fn expect_retry(decision: RetryDecision) -> Duration {
    match decision {
        RetryDecision::Retry { after } => after,
        RetryDecision::GiveUp(reason) => {
            panic!("expected Retry, got GiveUp({reason:?})")
        }
    }
}

// =============================================================================
// Full jitter — no server hint
// =============================================================================

#[test]
fn full_jitter_at_upper_bound_doubles_each_attempt() {
    let p = policy();
    // jitter_unit = 1.0 selects the top of the window, making the exponential
    // schedule directly observable.
    let cases = [(1, secs(1)), (2, secs(2)), (3, secs(4))];

    for (attempt, expected) in cases {
        let delay = expect_retry(decide(&p, attempt, None, Duration::ZERO, 1.0));
        assert_eq!(delay, expected, "attempt {attempt} window upper bound");
    }
}

#[test]
fn full_jitter_at_lower_bound_is_zero() {
    let p = policy();
    let delay = expect_retry(decide(&p, 1, None, Duration::ZERO, 0.0));
    assert_eq!(delay, Duration::ZERO);
}

#[test]
fn full_jitter_scales_linearly_within_the_window() {
    let p = policy();
    // Attempt 2 window is 2 s; half of it is 1 s.
    let delay = expect_retry(decide(&p, 2, None, Duration::ZERO, 0.5));
    assert_eq!(delay, secs(1));
}

#[test]
fn full_jitter_window_is_capped_by_max_backoff() {
    let p = RetryPolicy {
        max_attempts: 20,
        max_backoff: secs(3),
        ..policy()
    };
    // Attempt 5 would be 16 s uncapped; the cap holds it at 3 s.
    let delay = expect_retry(decide(&p, 5, None, Duration::ZERO, 1.0));
    assert_eq!(delay, secs(3));
}

#[test]
fn absurd_attempt_number_does_not_overflow() {
    let p = RetryPolicy {
        max_attempts: u32::MAX,
        max_backoff: secs(15),
        ..policy()
    };
    // The doubling is shift-based; this must saturate at the cap, not panic.
    let delay = expect_retry(decide(&p, u32::MAX - 1, None, Duration::ZERO, 1.0));
    assert_eq!(delay, secs(15));
}

// =============================================================================
// Server-supplied Retry-After
// =============================================================================

#[test]
fn server_hint_is_honoured_exactly_as_a_floor() {
    let p = policy();
    // Zero jitter means the delay is the server's value untouched — never
    // shorter, because waking early only burns an attempt.
    let delay = expect_retry(decide(&p, 1, Some(secs(5)), Duration::ZERO, 0.0));
    assert_eq!(delay, secs(5));
}

#[test]
fn server_hint_adds_decorrelating_jitter_on_top() {
    let p = policy();
    // Floor of 5 s plus up to `initial_backoff` (1 s) of spread.
    let delay = expect_retry(decide(&p, 1, Some(secs(5)), Duration::ZERO, 1.0));
    assert_eq!(delay, secs(6));
}

#[test]
fn absurd_server_hint_is_clamped_to_max_backoff() {
    let p = policy();
    // A day-long Retry-After must not park the request.
    let delay = expect_retry(decide(&p, 1, Some(secs(86_400)), Duration::ZERO, 0.0));
    assert_eq!(delay, p.max_backoff);
}

#[test]
fn server_hint_overrides_the_exponential_schedule() {
    let p = policy();
    // Attempt 3 would be a 4 s window on its own; the hint takes precedence.
    let delay = expect_retry(decide(&p, 3, Some(secs(2)), Duration::ZERO, 0.0));
    assert_eq!(delay, secs(2));
}

// =============================================================================
// Stopping conditions
// =============================================================================

#[test]
fn stops_once_attempts_are_exhausted() {
    let p = policy();
    let decision = decide(&p, p.max_attempts, None, Duration::ZERO, 0.0);
    assert_eq!(
        decision,
        RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted)
    );
}

#[test]
fn max_attempts_of_one_disables_retrying() {
    let p = RetryPolicy {
        max_attempts: 1,
        ..policy()
    };
    let decision = decide(&p, 1, None, Duration::ZERO, 0.0);
    assert_eq!(
        decision,
        RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted)
    );
}

#[test]
fn stops_once_the_deadline_has_passed() {
    let p = policy();
    let decision = decide(&p, 1, None, p.total_deadline, 0.0);
    assert_eq!(
        decision,
        RetryDecision::GiveUp(GiveUpReason::DeadlineExceeded)
    );
}

#[test]
fn stops_rather_than_sleeping_past_the_deadline() {
    let p = policy();
    // 59 s spent, a 1 s delay would land exactly on the 60 s deadline. Waking
    // early would be worse than stopping, so the policy stops.
    let decision = decide(&p, 1, None, secs(59), 1.0);
    assert_eq!(
        decision,
        RetryDecision::GiveUp(GiveUpReason::DeadlineExceeded)
    );
}

#[test]
fn retries_when_the_delay_fits_inside_the_deadline() {
    let p = policy();
    let delay = expect_retry(decide(&p, 1, None, secs(58), 1.0));
    assert_eq!(delay, secs(1));
}

#[test]
fn attempts_are_checked_before_the_deadline() {
    let p = policy();
    // Both limits are blown; the attempt count is the reported cause.
    let decision = decide(&p, p.max_attempts, None, secs(600), 0.0);
    assert_eq!(
        decision,
        RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted)
    );
}

// =============================================================================
// Defensive input handling
// =============================================================================

#[test]
fn out_of_range_jitter_is_clamped_to_the_window() {
    let p = policy();
    let cases = [
        (-1.0, Duration::ZERO, "negative clamps to the window floor"),
        (5.0, secs(1), "above one clamps to the window ceiling"),
        (f64::NAN, Duration::ZERO, "NaN falls back to no jitter"),
        (f64::INFINITY, Duration::ZERO, "infinity falls back to none"),
    ];

    for (jitter, expected, why) in cases {
        let delay = expect_retry(decide(&p, 1, None, Duration::ZERO, jitter));
        assert_eq!(delay, expected, "{why}");
    }
}

#[test]
fn give_up_reasons_have_stable_descriptions() {
    assert_eq!(
        GiveUpReason::AttemptsExhausted.as_str(),
        "attempts exhausted"
    );
    assert_eq!(GiveUpReason::DeadlineExceeded.as_str(), "deadline exceeded");
}

// =============================================================================
// Defaults
// =============================================================================

#[test]
fn default_policy_budget_stays_modest() {
    // The proxy already absorbs ModelLoading server-side, so this budget only
    // covers contention. Guard the numbers so a future edit is deliberate.
    let p = RetryPolicy::default();
    assert_eq!(p.max_attempts, 4);
    assert_eq!(p.total_deadline, secs(60));
    assert!(
        p.max_backoff < p.total_deadline,
        "a single delay must never be able to consume the whole budget"
    );
}
