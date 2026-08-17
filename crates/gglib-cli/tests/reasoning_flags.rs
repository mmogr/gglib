//! The CLI surface of the two reasoning controls.
//!
//! Split from `cli_parity.rs` — which guards that the *flag set* is the same
//! everywhere — because this suite asks a different question: that the values
//! land on the right field, and that the two refusals say the right thing.
//!
//! The refusals matter more than usual here. Neither control is observable in
//! readback (ADR 0007 finding 7a: nothing echoes either field in `/slots` or
//! `/props`), so a value that was quietly mangled at the flag would never be
//! contradicted by anything the user could look at afterwards. The error
//! message at parse time is the only place the truth gets told.

use clap::Parser;
use gglib_cli::{Cli, Commands, ModelCommand, SamplingArgs};
use gglib_core::domain::ReasoningEffort;

#[path = "support/flag_surface.rs"]
mod flag_surface;

use flag_surface::{long_flags_at, sampling_flags};

/// The sampling args a full command line resolved to.
fn sampling_of(argv: &[&str]) -> SamplingArgs {
    let cli = Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
    match cli.command {
        Some(Commands::Serve { sampling, .. } | Commands::Proxy { sampling, .. }) => sampling,
        Some(Commands::Chat { sampling, .. } | Commands::Question { sampling, .. }) => sampling,
        _ => panic!("unexpected command for {argv:?}"),
    }
}

/// The message clap refuses `argv` with.
///
/// `Cli` is not `Debug`, so `expect_err` is unavailable; and the message is
/// the whole point of these assertions anyway.
fn refusal(argv: &[&str]) -> String {
    match Cli::try_parse_from(argv) {
        Ok(_) => panic!("{argv:?} should have been refused"),
        Err(error) => error.to_string(),
    }
}

/// Every command that takes sampling flags must take these two, and put them
/// in the same place. The flag set is derived in `cli_parity`; what is checked
/// here is that the parsed value actually reaches the field.
#[test]
fn all_four_inference_commands_land_both_controls() {
    let invocations: [&[&str]; 4] = [
        &["gglib", "serve", "1"],
        &["gglib", "proxy"],
        &["gglib", "chat", "3"],
        &["gglib", "q", "hello"],
    ];

    for base in invocations {
        let mut argv = base.to_vec();
        argv.extend(["--reasoning-effort", "xhigh"]);
        argv.extend(["--reasoning-budget-tokens", "2048"]);

        let sampling = sampling_of(&argv);
        assert_eq!(
            sampling.reasoning_effort,
            Some(ReasoningEffort::XHigh),
            "for {base:?}"
        );
        assert_eq!(sampling.reasoning_budget_tokens, Some(2048), "for {base:?}");
    }
}

/// Both are `SamplingArgs` fields, so both must appear in the derived flag set
/// the parity guard compares against — this pins that they were added to the
/// shared group rather than inline on one command.
#[test]
fn both_controls_are_part_of_the_shared_sampling_group() {
    let flags = sampling_flags();
    assert!(flags.contains(&"reasoning-effort".to_owned()), "{flags:?}");
    assert!(
        flags.contains(&"reasoning-budget-tokens".to_owned()),
        "{flags:?}"
    );
}

#[test]
fn every_level_parses_to_its_own_variant() {
    for level in ReasoningEffort::ALL {
        let sampling = sampling_of(&["gglib", "proxy", "--reasoning-effort", level.as_str()]);
        assert_eq!(sampling.reasoning_effort, Some(level));
    }
}

/// `"none"` is upstream-valid and does the opposite of what it reads as: it
/// erases the kwarg, so the template's own default fires (medium, on
/// `gpt-oss`). Refusing it is only half the job — the message has to name the
/// flag that actually stops thinking, or the user simply retries with a
/// synonym.
#[test]
fn reasoning_effort_none_is_refused_and_redirected() {
    let message = refusal(&["gglib", "proxy", "--reasoning-effort", "none"]);

    assert!(
        message.contains("--reasoning-budget-tokens 0"),
        "should point at the flag that turns thinking off: {message}"
    );
    assert!(
        message.contains("medium"),
        "should say what 'none' would really have done: {message}"
    );
}

#[test]
fn an_unknown_effort_level_is_refused_with_the_vocabulary() {
    let message = refusal(&["gglib", "proxy", "--reasoning-effort", "banana"]);

    assert!(message.contains("minimal"), "{message}");
    assert!(message.contains("xhigh"), "{message}");
}

/// The two negative values are not interchangeable and the CLI must not treat
/// them as one: `-1` defers to the launch default and is valid, `-2` is
/// outside upstream's range and is not.
#[test]
fn the_budget_accepts_upstreams_range_and_refuses_below_it() {
    assert_eq!(
        sampling_of(&["gglib", "proxy", "--reasoning-budget-tokens", "-1"]).reasoning_budget_tokens,
        Some(-1)
    );
    assert_eq!(
        sampling_of(&["gglib", "proxy", "--reasoning-budget-tokens", "0"]).reasoning_budget_tokens,
        Some(0),
        "0 is valid and is how thinking is switched off"
    );

    let message = refusal(&["gglib", "proxy", "--reasoning-budget-tokens", "-2"]);
    assert!(
        message.contains("-1 defers to the launch default"),
        "should quote upstream's own range: {message}"
    );
}

/// `-1` is a *documented value* of this flag, so the spelling a user reaches
/// for first has to work. Without `allow_hyphen_values` clap reads the leading
/// `-` as the start of another flag and answers `unexpected argument '-1'`,
/// which tells them nothing about what to do instead.
#[test]
fn the_defer_sentinel_parses_separated_as_well_as_joined() {
    for argv in [
        vec!["gglib", "proxy", "--reasoning-budget-tokens", "-1"],
        vec!["gglib", "proxy", "--reasoning-budget-tokens=-1"],
    ] {
        assert_eq!(
            sampling_of(&argv).reasoning_budget_tokens,
            Some(-1),
            "for {argv:?}"
        );
    }
}

/// The cost of `allow_hyphen_values` is that a following flag can be eaten as
/// this one's value. The value parser is what stops that being silent: the
/// mistake surfaces as the range error rather than as a run with the wrong
/// settings.
#[test]
fn a_flag_swallowed_as_the_budget_value_is_still_refused() {
    let message = refusal(&["gglib", "proxy", "--reasoning-budget-tokens", "--cache"]);
    assert!(
        message.contains("-1 defers to the launch default"),
        "got: {message}"
    );
}

/// An all-`None` config is what tells the merge hierarchy the user expressed
/// no opinion, so `into_override` must still return `Some` when the *only*
/// flag passed was a reasoning one. Getting this wrong would drop the value
/// silently — and nothing downstream echoes it to contradict that.
#[test]
fn a_reasoning_flag_alone_still_counts_as_an_override() {
    for argv in [
        ["gglib", "proxy", "--reasoning-effort", "low"],
        ["gglib", "proxy", "--reasoning-budget-tokens", "0"],
    ] {
        let config = sampling_of(&argv)
            .into_override()
            .unwrap_or_else(|| panic!("{argv:?} expresses an opinion"));
        assert!(
            config.reasoning_effort.is_some() || config.reasoning_budget_tokens.is_some(),
            "for {argv:?}"
        );
    }

    assert!(
        sampling_of(&["gglib", "proxy"]).into_override().is_none(),
        "a bare invocation still expresses nothing"
    );
}

// ─── The stored twins ─────────────────────────────────────────────────────

#[test]
fn model_update_stores_both_controls_and_clears_one() {
    let cli = Cli::try_parse_from([
        "gglib",
        "model",
        "update",
        "3",
        "--reasoning-effort",
        "high",
        "--reasoning-budget-tokens",
        "16384",
        "--unset",
        "temperature",
        "--unset",
        "top_k",
    ])
    .expect("model update should accept the reasoning twins and --unset");

    let Some(Commands::Model {
        command:
            ModelCommand::Update {
                reasoning_effort,
                reasoning_budget_tokens,
                unset,
                ..
            },
    }) = cli.command
    else {
        panic!("expected a Model::Update command");
    };

    assert_eq!(reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(reasoning_budget_tokens, Some(16384));
    assert_eq!(unset, ["temperature", "top_k"], "repeatable, in order");
}

/// The stored twins are refused by the same parser as the per-run flags, so
/// `--reasoning-effort none` cannot be *saved* either.
#[test]
fn the_stored_twins_refuse_what_the_per_run_flags_refuse() {
    for argv in [
        vec![
            "gglib",
            "model",
            "update",
            "3",
            "--reasoning-effort",
            "none",
        ],
        vec![
            "gglib",
            "model",
            "update",
            "3",
            "--reasoning-budget-tokens",
            "-2",
        ],
        vec![
            "gglib",
            "config",
            "profile",
            "set",
            "deep",
            "--reasoning-effort",
            "none",
        ],
    ] {
        refusal(&argv);
    }
}

#[test]
fn config_profile_set_takes_both_controls() {
    let flags = long_flags_at(&["config", "profile", "set"]);

    for flag in ["reasoning-effort", "reasoning-budget-tokens", "unset"] {
        assert!(
            flags.contains(&flag.to_owned()),
            "missing --{flag}: {flags:?}"
        );
    }
}
