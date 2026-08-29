//! The `GGLIB_DISABLE_*` environment switches, and which are in effect here.
//!
//! Each one turns off a compensation so its effect can be measured — the
//! deletion-criterion discipline [ADR 0001] describes, reachable at runtime
//! without a rebuild.
//!
//! # Why they need a roster
//!
//! Every one of them is read with [`std::env::var`] **in the process that does
//! the work**, and for gglib that process is the daemon. So
//!
//! ```text
//! GGLIB_DISABLE_AGENTIC_SAMPLING=1 gglib benchmark agentic --model m
//! ```
//!
//! sets the variable on a CLI process which resolves no sampling at all. The
//! daemon does, and it never saw it. The switch is silently ignored, the run
//! completes, and the numbers look like an answer.
//!
//! That is not a hypothetical: it produced two identical arms of an A/B eval
//! that were meant to differ, and the only reason it was caught is that the
//! arms were checked against `/proc/<pid>/environ` before the run rather than
//! after it. A debugging switch whose failure mode is "quietly changes
//! nothing" is worse than no switch, because it manufactures confident wrong
//! conclusions from real work.
//!
//! So the daemon reports which of these it actually has in effect, and the CLI
//! compares that against its own environment before handing a command over.
//! A mismatch is stated, not swallowed.
//!
//! # Why a name list and not typed constants
//!
//! The switches live in three crates — `request_pipeline` here,
//! `canonicalization`/`repair` in `gglib-proxy`, `command`/`cache_ram`/
//! `kv_cache_type` in `gglib-runtime` — and this crate sits below all of them.
//! Names are the only thing they share. [`ALL`] is therefore a hand-written
//! list, which is exactly the shape that rotted in `model_service`'s
//! `AUTO_TAG_NAMES`, so `all_lists_every_switch_the_tree_reads` greps the
//! source tree and fails if the two disagree in either direction.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md

use std::fmt::Write as _;

/// Every `GGLIB_DISABLE_*` switch the tree reads.
///
/// Kept sorted so the reported order is stable across processes — a mismatch
/// warning that reorders itself is harder to read than one that does not.
pub const ALL: &[&str] = &[
    "GGLIB_DISABLE_AGENTIC_SAMPLING",
    "GGLIB_DISABLE_CACHE_AUTOSIZE",
    "GGLIB_DISABLE_CACHE_REUSE",
    "GGLIB_DISABLE_CONTEXT_FIT",
    "GGLIB_DISABLE_GRAMMAR",
    "GGLIB_DISABLE_KV_QUANT",
    "GGLIB_DISABLE_MTP",
    "GGLIB_DISABLE_PROMPT_CANONICALIZATION",
    "GGLIB_DISABLE_TOOL_REPAIR",
];

/// Whether an environment value reads as "on".
///
/// The spelling every switch in the tree already accepts, gathered here so a
/// ninth one cannot quietly accept a different set. That claim used to be
/// false: five sites had their own copy of this `matches!`, two of them in
/// this very crate, and this one had no callers outside its own module.
#[must_use]
pub fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether the named environment switch is set to a truthy value **now**.
///
/// Reads the live environment on every call rather than caching. A switch
/// consulted once at startup would ignore a `.env` loaded later, and the tests
/// that set one per case would leak into each other.
#[must_use]
pub fn enabled(var: &str) -> bool {
    std::env::var(var).ok().is_some_and(|v| is_truthy(&v))
}

/// The switches set truthy **in this process**.
///
/// Reads the live environment, so a caller in the CLI gets the CLI's answer
/// and one in the daemon gets the daemon's. That difference is the whole
/// point — see the module docs.
#[must_use]
pub fn active() -> Vec<&'static str> {
    ALL.iter().copied().filter(|name| enabled(name)).collect()
}

/// What the CLI should say when its switches differ from the daemon's.
///
/// `None` when they agree. Returned rather than printed so the caller owns the
/// output stream and this stays testable.
#[must_use]
pub fn describe_mismatch(here: &[&str], daemon: &[String]) -> Option<String> {
    let ignored: Vec<&str> = here
        .iter()
        .copied()
        .filter(|n| !daemon.iter().any(|d| d == n))
        .collect();
    let unexpected: Vec<&String> = daemon
        .iter()
        .filter(|d| !here.contains(&d.as_str()))
        .collect();

    if ignored.is_empty() && unexpected.is_empty() {
        return None;
    }

    let mut out = String::from("debug switches differ between this command and the daemon\n");
    if !ignored.is_empty() {
        let _ = writeln!(
            out,
            "  set here but NOT in effect: {}\n  \
             The daemon does the work, so these are being ignored.",
            ignored.join(", ")
        );
    }
    if !unexpected.is_empty() {
        let names: Vec<&str> = unexpected.iter().map(|s| s.as_str()).collect();
        let _ = writeln!(
            out,
            "  in effect in the daemon but not set here: {}",
            names.join(", ")
        );
    }
    out.push_str("  Restart the daemon to apply them: `gglib daemon stop`, then re-run.");
    Some(out)
}

#[cfg(test)]
#[path = "debug_switches_tests.rs"]
mod tests;
