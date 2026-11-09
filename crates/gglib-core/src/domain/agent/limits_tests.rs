//! A turn's limits: what it names, then what this machine stores, then the
//! built-in default.

use super::{ToolLimits, TurnLimits};
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

/// Settings that store the two tool limits and nothing else.
fn storing_tools(parallel: Option<u32>, timeout_ms: Option<u64>) -> Settings {
    Settings {
        max_parallel_tools: parallel,
        tool_timeout_ms: timeout_ms,
        ..Settings::default()
    }
}

/// Each row is what the turn named, what is stored, and the limit it runs
/// with: the turn's own, then the stored one, then none (the built-in
/// default).
#[test]
fn each_tool_limit_is_the_turns_own_then_the_stored_one_then_the_default() {
    let table: [(Option<u32>, Option<u32>, Option<u32>); 4] = [
        (Some(3), Some(40), Some(3)),
        (Some(3), None, Some(3)),
        (None, Some(40), Some(40)),
        (None, None, None),
    ];
    for (named, stored, want) in table {
        let parallel = ToolLimits::resolve(
            named.map(|n| n as usize),
            None,
            Some(&storing_tools(stored, None)),
        );
        assert_eq!(
            parallel.max_parallel_tools,
            want.map(|n| n as usize),
            "parallel: named {named:?}, stored {stored:?}"
        );
        let named_ms = named.map(u64::from);
        let stored_ms = stored.map(u64::from);
        let timeout = ToolLimits::resolve(None, named_ms, Some(&storing_tools(None, stored_ms)));
        assert_eq!(
            timeout.tool_timeout_ms,
            want.map(u64::from),
            "timeout: named {named_ms:?}, stored {stored_ms:?}"
        );
    }
}

/// The stored limits reach the loop's config, clamped where it reads them.
#[test]
fn stored_tool_limits_reach_the_loops_config_clamped() {
    let limits = ToolLimits::resolve(None, None, Some(&storing_tools(Some(7), Some(1_000_000))));
    let config = AgentConfig::from_user_params(
        None,
        limits.max_parallel_tools,
        limits.tool_timeout_ms,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(config.max_parallel_tools, 7);
    assert_eq!(config.tool_timeout_ms, 60_000);
}
