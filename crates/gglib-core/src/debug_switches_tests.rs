//! Tests for [`super`] — the debug-switch registry.
//!
//! Their own file, the way `models_tests.rs` is: the module is a short list of
//! constants and three small functions, and the tests are longer than all of
//! it, because one of them walks the crate tree.

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

/// The claim in `is_truthy`'s doc — that this is the spelling every switch
/// accepts — is only true if nobody writes their own. Five sites did, two
/// of them in this crate, and nothing failed when they diverged because
/// each one was locally correct.
///
/// So this greps the tree. A unit test cannot see a copy that has not been
/// written yet; a search for the literal can.
#[test]
fn nothing_else_spells_out_the_truthy_set() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .to_path_buf();

    let mut offenders = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // Built from parts so this test's own source does not match.
                let needle = format!("{:?} | {:?} | {:?} | {:?}", "1", "true", "yes", "on");
                if text.contains(&needle) && !path.ends_with("gglib-core/src/debug_switches.rs") {
                    offenders.push(path);
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the truthy set is spelled out outside debug_switches: {offenders:#?}\n\
         Call `debug_switches::is_truthy` (or `enabled`) instead."
    );
}
