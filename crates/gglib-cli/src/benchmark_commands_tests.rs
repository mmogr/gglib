//! The `--seeds` surface of `gglib benchmark agentic`.
//!
//! Split from `benchmark_commands.rs` per the repo's `*_tests.rs` sibling
//! pattern, and testing at the CLI rather than below it on purpose: every
//! layer beneath this flag was already correct when it broke. `plan_arms`
//! reads an empty seed list as "run once, naming no seed", and the handler
//! preserved emptiness deliberately — but `--seeds ""`, the form the help
//! text advertised, could not be expressed at all. `value_delimiter = ','`
//! splits the raw `""` into one empty *element* rather than zero elements,
//! so a plain `u32` parser rejected it before any of that logic ran.
//!
//! These assert the whole path an argv takes to the seed list an eval
//! actually runs, because the defect lived in the gap between the two.

use super::*;
use crate::{Cli, Commands};
use clap::Parser;

/// The seeds a full command line resolves to.
fn seeds_of(argv: &[&str]) -> Vec<u32> {
    let cli = Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
    match cli.command {
        Some(Commands::Benchmark {
            command: BenchmarkCommand::Agentic { seeds, .. },
        }) => resolve_seeds(seeds),
        _ => panic!("unexpected command for {argv:?}"),
    }
}

/// The argv every case here extends. `-m` is required, nothing else is.
fn argv(extra: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = ["gglib", "benchmark", "agentic", "-m", "2"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    v.extend(extra.iter().map(|s| (*s).to_string()));
    v
}

fn seeds_with(extra: &[&str]) -> Vec<u32> {
    let owned = argv(extra);
    let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
    seeds_of(&borrowed)
}

/// The regression guard. `--seeds ""` is what the help text tells users to
/// write for a single unseeded run, and for as long as the value parser was
/// a bare `u32` it exited with "cannot parse integer from empty string" —
/// documented, reached for, and impossible.
#[test]
fn an_empty_seeds_argument_asks_for_one_unseeded_run() {
    assert_eq!(
        seeds_with(&["--seeds", ""]),
        Vec::<u32>::new(),
        "`--seeds \"\"` must resolve to no seeds, which plan_arms runs once unseeded"
    );
}

/// `num_args = 0..` also admits the flag with no value at all. It works and
/// is pinned here, but the help text points at `--seeds \"\"` instead: this
/// form sits on a variable-arity boundary and is fragile to argument order.
#[test]
fn a_bare_seeds_flag_also_asks_for_one_unseeded_run() {
    assert_eq!(seeds_with(&["--seeds"]), Vec::<u32>::new());
}

/// Emptiness has to mean "none", not "unspecified" — falling back to the
/// default here is what would quietly turn a requested 2-run smoke test back
/// into a 6-run one, and the user would be told nothing.
#[test]
fn an_empty_seed_list_is_not_the_same_as_an_absent_one() {
    let defaults = seeds_with(&[]);
    assert_eq!(
        defaults,
        DEFAULT_SEEDS.to_vec(),
        "an unpassed --seeds keeps the three defaults"
    );
    assert_ne!(
        seeds_with(&["--seeds", ""]),
        defaults,
        "an empty --seeds must not fall back to the default it explicitly overrides"
    );
}

#[test]
fn named_seeds_come_through_in_order() {
    assert_eq!(seeds_with(&["--seeds", "1,2,3"]), vec![1, 2, 3]);
    assert_eq!(seeds_with(&["--seeds", "12345"]), vec![12345]);
}

/// Empty means "no seed"; malformed still has to be an error. Mapping the
/// empty case to `None` inside the value parser rather than accepting any
/// unparseable string is what keeps this a clap failure at parse time —
/// before the handler's model lookup has touched the database.
#[test]
fn a_malformed_seed_is_still_rejected() {
    let owned = argv(&["--seeds", "abc"]);
    let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
    // `Cli` has no `Debug`, so `expect_err` is unavailable here.
    let msg = match Cli::try_parse_from(&borrowed) {
        Ok(_) => panic!("`--seeds abc` must not parse"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("abc"),
        "the error should name the value it rejected, got: {msg}"
    );
}

/// Whitespace is the near-miss of the empty form — a shell that expands to
/// `" "` should get the unseeded run it plainly meant, not an integer error.
#[test]
fn a_whitespace_only_seed_is_treated_as_empty() {
    assert_eq!(seeds_with(&["--seeds", "  "]), Vec::<u32>::new());
}
