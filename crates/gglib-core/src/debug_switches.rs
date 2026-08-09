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
    "GGLIB_DISABLE_GRAMMAR",
    "GGLIB_DISABLE_KV_QUANT",
    "GGLIB_DISABLE_MTP",
    "GGLIB_DISABLE_PROMPT_CANONICALIZATION",
    "GGLIB_DISABLE_TOOL_REPAIR",
];

/// Whether an environment value reads as "on".
///
/// The spelling every switch in the tree already accepts, gathered here so a
/// ninth one cannot quietly accept a different set.
#[must_use]
pub fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// The switches set truthy **in this process**.
///
/// Reads the live environment, so a caller in the CLI gets the CLI's answer
/// and one in the daemon gets the daemon's. That difference is the whole
/// point — see the module docs.
#[must_use]
pub fn active() -> Vec<&'static str> {
    ALL.iter()
        .copied()
        .filter(|name| std::env::var(name).ok().is_some_and(|v| is_truthy(&v)))
        .collect()
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
mod tests {
    use super::*;

    #[test]
    fn truthy_values_are_the_ones_the_tree_already_accepted() {
        for v in ["1", "true", "yes", "on", "TRUE", " On "] {
            assert!(is_truthy(v), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", "", "maybe"] {
            assert!(!is_truthy(v), "{v:?} should not be truthy");
        }
    }

    /// [`ALL`] must name every `GGLIB_DISABLE_*` the tree reads, in both
    /// directions.
    ///
    /// A hand-written roster of strings that other crates own is precisely the
    /// shape that rotted in `AUTO_TAG_NAMES`: a switch missing here is
    /// invisible to the mismatch warning forever, and one listed here that
    /// nothing reads reports a difference that cannot matter. So the source
    /// tree is the reference, read at test time — the same trick
    /// `sampler_wire_semantics.py` uses on the floor and
    /// `settingsBounds.test.ts` uses on the bounds.
    #[test]
    fn all_lists_every_switch_the_tree_reads() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/")
            .to_path_buf();

        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    // This file is the roster itself; its own list is not
                    // evidence that anything reads the switch.
                    if path.ends_with("debug_switches.rs") {
                        continue;
                    }
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    for (i, _) in text.match_indices("GGLIB_DISABLE_") {
                        let tail: String = text[i..]
                            .chars()
                            .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                            .collect();
                        if tail.len() > "GGLIB_DISABLE_".len() && !found.contains(&tail) {
                            found.push(tail);
                        }
                    }
                }
            }
        }

        assert!(
            !found.is_empty(),
            "the scan found no switches at all — it is not reading the tree, so its \
             agreement below would mean nothing"
        );

        for name in &found {
            assert!(
                ALL.contains(&name.as_str()),
                "{name} is read somewhere in the tree but missing from ALL, so a command \
                 setting it against a running daemon gets no warning"
            );
        }
        for name in ALL {
            assert!(
                found.iter().any(|f| f == name),
                "ALL lists {name}, which nothing in the tree reads"
            );
        }
    }

    #[test]
    fn agreeing_switch_sets_produce_no_warning() {
        assert!(describe_mismatch(&[], &[]).is_none());
        assert!(
            describe_mismatch(
                &["GGLIB_DISABLE_GRAMMAR"],
                &["GGLIB_DISABLE_GRAMMAR".to_string()]
            )
            .is_none()
        );
    }

    /// The case that cost an hour: set on the CLI, absent in the daemon.
    #[test]
    fn a_switch_the_daemon_never_saw_is_named_as_ignored() {
        let msg = describe_mismatch(&["GGLIB_DISABLE_AGENTIC_SAMPLING"], &[])
            .expect("a mismatch must be reported");

        assert!(msg.contains("GGLIB_DISABLE_AGENTIC_SAMPLING"), "{msg}");
        assert!(msg.contains("being ignored"), "{msg}");
        assert!(msg.contains("gglib daemon stop"), "{msg}");
    }

    /// The reverse: a daemon started with a switch the current command does
    /// not set. Equally worth saying — the run is not the vanilla one the
    /// operator thinks they are getting.
    #[test]
    fn a_switch_only_the_daemon_has_is_also_reported() {
        let msg = describe_mismatch(&[], &["GGLIB_DISABLE_TOOL_REPAIR".to_string()])
            .expect("a mismatch must be reported");

        assert!(msg.contains("GGLIB_DISABLE_TOOL_REPAIR"), "{msg}");
        assert!(msg.contains("in effect in the daemon"), "{msg}");
    }
}
