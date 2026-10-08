//! A turn's limits: what it names, then what this machine stores, then the
//! built-in default.

use super::TurnLimits;
use crate::domain::agent::{AgentConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_STAGNATION_STEPS};
use crate::settings::Settings;

/// Settings that store these two limits and nothing else.
fn storing(iterations: Option<u32>, stagnation: Option<u32>) -> Settings {
    Settings {
        max_tool_iterations: iterations,
        max_stagnation_steps: stagnation,
        ..Settings::default()
    }
}

/// Each row is what the turn named, what is stored, and the limit it runs
/// with.
#[test]
fn the_iteration_limit_is_the_turns_own_then_the_stored_one_then_the_default() {
    let table = [
        (Some(3), Some(40), 3),
        (Some(3), None, 3),
        (None, Some(40), 40),
        (None, None, DEFAULT_MAX_ITERATIONS),
    ];
    for (named, stored, want) in table {
        let settings = storing(stored, None);
        assert_eq!(
            TurnLimits::resolve(named, Some(&settings)).max_iterations,
            want,
            "named {named:?}, stored {stored:?}"
        );
    }
}

#[test]
fn the_stagnation_limit_is_the_stored_one_whatever_the_turn_names() {
    let settings = storing(Some(40), Some(9));
    for named in [None, Some(3)] {
        assert_eq!(
            TurnLimits::resolve(named, Some(&settings)).max_stagnation_steps,
            Some(9)
        );
    }
    assert_eq!(
        TurnLimits::resolve(None, Some(&storing(Some(40), None))).max_stagnation_steps,
        None
    );
}

/// Settings that could not be read leave what the turn named, and the
/// defaults for the rest.
#[test]
fn unread_settings_leave_the_turns_own_limit_and_the_defaults() {
    assert_eq!(
        TurnLimits::resolve(Some(3), None),
        TurnLimits {
            max_iterations: 3,
            max_stagnation_steps: None
        }
    );
    let limits = TurnLimits::resolve(None, None);
    let config = AgentConfig::from_user_params(
        Some(limits.max_iterations),
        None,
        None,
        None,
        None,
        limits.max_stagnation_steps,
    )
    .unwrap();
    assert_eq!(config.max_iterations, DEFAULT_MAX_ITERATIONS);
    assert_eq!(
        config.max_stagnation_steps,
        Some(DEFAULT_MAX_STAGNATION_STEPS)
    );
}
