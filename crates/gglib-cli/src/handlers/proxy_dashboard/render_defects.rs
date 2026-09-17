//! The per-model signals section of the frame.
//!
//! Its own file rather than more of `render`, because it is the one section
//! whose rules are about what *not* to print: a model with nothing wrong earns
//! no block, a clean run says so with its denominator, and a run that has
//! forwarded nothing says that instead. Those rules are most of the section, and
//! most of its tests.
//!
//! Pure, like its siblings: data in, `String` out, no IO and no clock.

use std::collections::BTreeMap;

use super::render::{thousands, truncate};
use super::wire::ModelDefectCounts;

/// Render the per-model signals section — what failed, what merely went in
/// circles, and for which model.
///
/// These are the diagnostic counters ADR 0006 kept when the tuning scheduler
/// went. Nothing acts on them automatically, which is exactly why they need
/// somewhere to be read: until now they were on `/v1/proxy/status` and in no
/// human-facing surface at all, so the only way to see them was `curl | jq`.
///
/// Only models with something to report get a line, and listing every clean
/// model would bury the one that is not.
///
/// Three counters here are not failures. `identical_result_repeats` describes a
/// conversation that went in a circle, and `repeats_not_evaluated` says how
/// often that question could not be answered — facts about the client's
/// history rather than faults in the model. `repeats_rescued` is a fact about
/// gglib instead: how often the loop guard declined to act because the answer
/// had moved. They print below the defects under an `observed` heading of their
/// own, and a model whose only signal is one of them still earns a line. The
/// section is named for signals rather than defects because of them.
pub(super) fn render_defects_section(per_model: &BTreeMap<String, ModelDefectCounts>) -> String {
    let mut out = String::from("Per-model signals (this proxy run)\n");

    let faulty: Vec<_> = per_model
        .iter()
        .filter(|(_, counts)| !counts.is_clean())
        .collect();

    if per_model.is_empty() {
        out.push_str("  (nothing recorded yet)\n");
        return out;
    }
    if faulty.is_empty() {
        let served: u64 = per_model.values().map(|c| c.requests).sum();
        out.push_str(&format!(
            "  none across {} request(s), {} model(s)\n",
            thousands(served),
            per_model.len()
        ));
        return out;
    }

    for (model, counts) in faulty {
        out.push_str(&format!(
            "  {:<28} {} request(s)\n",
            truncate(model, 28),
            thousands(counts.requests)
        ));

        // Repairs read as a ratio: the attempt rate says how often this model
        // packages a tool call badly, and the success rate says whether the
        // one lever gglib pulls is working on it.
        if counts.repairs_attempted > 0 {
            out.push_str(&format!(
                "    {:<24} {} of {} succeeded\n",
                "tool-call repairs",
                thousands(counts.repairs_succeeded),
                thousands(counts.repairs_attempted)
            ));
        }

        for (label, value) in [
            ("loop-guard trips", counts.loop_guard_trips),
            ("stream errors", counts.stream_errors),
            ("truncated at ceiling", counts.truncated_generations),
            ("dialect residue", counts.dialect_residue),
            ("unvalidatable schemas", counts.unvalidatable_schemas),
            ("normalization errors", counts.normalization_errors),
        ] {
            if value > 0 {
                out.push_str(&format!("    {label:<24} {}\n", thousands(value)));
            }
        }

        // reasoning_only is counted *within* empty_responses, so it is shown
        // as a share of them rather than beside them — printing both as peers
        // reads as more empty turns than actually happened.
        if counts.empty_responses > 0 {
            if counts.reasoning_only > 0 {
                out.push_str(&format!(
                    "    {:<24} {} ({} reasoning-only)\n",
                    "empty responses",
                    thousands(counts.empty_responses),
                    thousands(counts.reasoning_only)
                ));
            } else {
                out.push_str(&format!(
                    "    {:<24} {}\n",
                    "empty responses",
                    thousands(counts.empty_responses)
                ));
            }
        }

        // Below the defects, under a heading of their own, because neither is
        // one: the model asked for the same thing twice and the environment
        // gave the same answer twice. Nothing acts on them — they are the
        // evidence for whether a corrective arm on the input plane would ever
        // fire, and the second says how often the question could be asked at
        // all. Own indent level so the label column is not shared with the
        // defect rows above.
        if counts.identical_result_repeats > 0
            || counts.repeats_not_evaluated > 0
            || counts.repeats_rescued > 0
        {
            out.push_str("    observed\n");
            for (label, value) in [
                ("repeated, same result", counts.identical_result_repeats),
                ("repeated, not comparable", counts.repeats_not_evaluated),
                ("repeated, new result", counts.repeats_rescued),
            ] {
                if value > 0 {
                    out.push_str(&format!("      {label:<24} {}\n", thousands(value)));
                }
            }
        }
    }

    out
}

#[cfg(test)]
#[path = "render_defects_tests.rs"]
mod tests;
