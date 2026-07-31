//! Environment-override tests.
//!
//! Overrides are applied through an injected lookup rather than by mutating the
//! process environment, which is global and would race the other tests sharing
//! this binary.

use std::collections::HashMap;

use super::*;
use crate::retry::{GiveUpReason, RetryDecision, decide};

/// A stand-in for `std::env::var` over a fixed set of variables.
fn fixture(pairs: &[(&str, &str)]) -> impl Fn(&'static str) -> Result<String, ()> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name: &'static str| map.get(name).cloned().ok_or(())
}

fn overridden(pairs: &[(&str, &str)]) -> RetryPolicy {
    RetryPolicy::default().with_env_overrides(&fixture(pairs))
}

#[test]
fn an_empty_environment_leaves_the_defaults_alone() {
    assert_eq!(overridden(&[]), RetryPolicy::default());
}

#[test]
fn max_attempts_is_overridden() {
    let policy = overridden(&[(MAX_ATTEMPTS_ENV_VAR, "9")]);
    assert_eq!(policy.max_attempts, 9);
    assert_eq!(
        policy.total_deadline,
        RetryPolicy::default().total_deadline,
        "an unset variable must not disturb the other field"
    );
}

#[test]
fn the_deadline_is_overridden() {
    let policy = overridden(&[(DEADLINE_ENV_VAR, "120")]);
    assert_eq!(policy.total_deadline, Duration::from_mins(2));
    assert_eq!(policy.max_attempts, RetryPolicy::default().max_attempts);
}

#[test]
fn both_can_be_set_together() {
    let policy = overridden(&[(MAX_ATTEMPTS_ENV_VAR, "2"), (DEADLINE_ENV_VAR, "10")]);
    assert_eq!(policy.max_attempts, 2);
    assert_eq!(policy.total_deadline, Duration::from_secs(10));
}

#[test]
fn surrounding_whitespace_is_tolerated() {
    assert_eq!(
        overridden(&[(MAX_ATTEMPTS_ENV_VAR, "  6 ")]).max_attempts,
        6
    );
}

#[test]
fn zero_attempts_is_read_as_one() {
    // Never issuing the request at all is nobody's intent; "off" means one
    // attempt, which is what `--no-retry` also resolves to.
    assert_eq!(overridden(&[(MAX_ATTEMPTS_ENV_VAR, "0")]).max_attempts, 1);
}

#[test]
fn an_unusable_value_degrades_to_the_default() {
    // A typo must not silently disable retrying.
    for bad in ["", "lots", "-1", "3.5"] {
        let policy = overridden(&[(MAX_ATTEMPTS_ENV_VAR, bad)]);
        assert_eq!(
            policy.max_attempts,
            RetryPolicy::default().max_attempts,
            "value {bad:?} should have been ignored"
        );
    }
}

#[test]
fn a_shortened_deadline_pulls_the_backoff_ceiling_down_with_it() {
    // Without this, a 10 s budget against the default 15 s ceiling would let the
    // very first backoff overrun the deadline — silently turning retry off for
    // an operator who thought they were only tightening it.
    let policy = overridden(&[(DEADLINE_ENV_VAR, "10")]);

    assert!(
        policy.max_backoff <= policy.total_deadline / 2,
        "a single delay must never be able to consume the whole budget"
    );

    // Prove the consequence, not just the arithmetic: a retry still happens.
    let decision = decide(&policy, 1, None, Duration::ZERO, 1.0);
    assert!(
        matches!(decision, RetryDecision::Retry { .. }),
        "a tightened budget must still allow at least one retry, got {decision:?}"
    );
}

#[test]
fn a_lengthened_deadline_leaves_the_ceiling_alone() {
    let policy = overridden(&[(DEADLINE_ENV_VAR, "600")]);
    assert_eq!(policy.max_backoff, RetryPolicy::default().max_backoff);
}

#[test]
fn disabled_makes_exactly_one_attempt() {
    let policy = RetryPolicy::disabled();
    assert_eq!(policy.max_attempts, 1);
    assert_eq!(
        decide(&policy, 1, None, Duration::ZERO, 0.0),
        RetryDecision::GiveUp(GiveUpReason::AttemptsExhausted),
        "--no-retry must not back off even once"
    );
}

#[test]
fn the_env_var_names_are_stable() {
    // Documented in the module README and the operator note.
    assert_eq!(MAX_ATTEMPTS_ENV_VAR, "GGLIB_LLM_RETRY_MAX_ATTEMPTS");
    assert_eq!(DEADLINE_ENV_VAR, "GGLIB_LLM_RETRY_DEADLINE_SECS");
}
