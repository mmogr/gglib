//! The reasoning-control section of the frame.
//!
//! Its own file rather than more of [`super::render`], because it is the one
//! section that is **not a readback**. Every other part of this dashboard
//! reports something the proxy measured; llama-server echoes neither reasoning
//! control anywhere gglib can read (ADR 0007 finding 7a), so these lines print
//! gglib's own record of what it resolved and say so in the server's words.
//!
//! Pure, like its sibling: data in, `String` out, no IO and no clock.

use super::render::{thousands, truncate};
use super::wire_sampling::{EffortSupport, SamplingAudit};

/// Render the reasoning controls: what the running template says about
/// `reasoning_effort`, what the last request resolved, and why none of it is an
/// observation.
///
/// The only section of this dashboard that reports values **nothing echoes**.
/// llama-server serialises neither reasoning control anywhere gglib can read it
/// (ADR 0007 finding 7a), so where every other section compares two sides, this
/// one prints gglib's record and says so — the server sends the sentence, and
/// it is printed beside a value rather than permanently, because a warning that
/// qualifies nothing is a warning people learn to skip.
pub(super) fn render_reasoning_section(audit: Option<&SamplingAudit>, term_width: u16) -> String {
    let mut out = String::from("Reasoning controls\n");
    let Some(reasoning) = audit.and_then(|a| a.reasoning.as_ref()) else {
        out.push_str("  (not reported by this proxy)\n");
        return out;
    };

    // Clip to one physical row, matching how every other server-phrased string
    // in this frame is handled.
    let max_chars = usize::from(term_width.saturating_sub(4));
    out.push_str(&format!(
        "  {:<24} {}\n",
        "Template reads effort",
        truncate(&effort_support_label(&reasoning.effort_support), max_chars)
    ));

    let Some(latest) = reasoning.latest.as_ref() else {
        out.push_str("  (no request has resolved either control yet)\n");
        return out;
    };

    match latest.effort.as_ref() {
        None => out.push_str(&format!("  {:<24} {}\n", "Effort", "none resolved")),
        Some(rung) => {
            let suppressed = if rung.suppressed {
                " — suppressed, this template never reads it"
            } else {
                ""
            };
            out.push_str(&format!(
                "  {:<24} {} ({}){}\n",
                "Effort", rung.level, rung.source, suppressed
            ));
        }
    }
    match latest.budget.as_ref() {
        None => out.push_str(&format!("  {:<24} {}\n", "Budget", "none resolved")),
        Some(rung) => out.push_str(&format!(
            "  {:<24} {} tokens ({})\n",
            "Budget",
            budget_tokens(rung.tokens),
            rung.source
        )),
    }

    if latest.effort.is_some() || latest.budget.is_some() {
        out.push_str(&format!(
            "  ! {}\n",
            truncate(&reasoning.wire_blind_reason, max_chars)
        ));
    }
    out
}

/// Group a resolved budget's digits without losing its sign.
///
/// The budget is an `i32` because two of its values are negative-or-zero
/// sentinels carrying **opposite instructions**: `-1` defers to the launch
/// `--reasoning-budget`, and `0` stops thinking immediately (see
/// `gglib_core::domain::inference::read_reasoning_budget_tokens`, which accepts
/// `-1..=i32::MAX`, and `reasoning_args::parse_budget`, which lets `-1` through
/// from the flag into profiles and global config). Rendering through
/// [`thousands`], which takes a `u64`, therefore cannot be done with a
/// saturating conversion: `-1` would print as `0`, i.e. as the other sentinel,
/// on the one surface this section exists to be — gglib's own record, with no
/// readback anywhere that could contradict it.
///
/// Printed raw and signed, matching `explain_display::fmt_i32` and the GUI's
/// `budget.tokens.toLocaleString()`. The sentinel meanings are deliberately not
/// glossed here, for the reason `explain_display::fmt_effort` gives: if this row
/// ever gains that commentary, it belongs to the row, not to the formatter.
fn budget_tokens(tokens: i32) -> String {
    let magnitude = thousands(u64::from(tokens.unsigned_abs()));
    if tokens < 0 {
        format!("-{magnitude}")
    } else {
        magnitude
    }
}

/// One line for the template's answer, keeping "not observed" out of "no".
///
/// The tri-state's whole point: a template that positively does not read the
/// variable and one nobody has managed to ask are different facts, and only the
/// first licenses concluding that an effort setting is inert.
fn effort_support_label(support: &EffortSupport) -> String {
    match support {
        EffortSupport::Supported => "yes".to_string(),
        EffortSupport::NotSupported => {
            "no — a resolved effort is suppressed before sending".to_string()
        }
        EffortSupport::NotYetObserved { reason } => format!("not observed — {reason}"),
        EffortSupport::Unrecognised => {
            "not observed — this proxy reported a state this build does not recognise".to_string()
        }
    }
}

/// Render which of the client's own sampling fields were dropped, by name.
///
/// The count alone ("4 fields discarded") cannot answer the question this
/// record exists for — *"is gglib ignoring the `reasoning_effort` I sent?"* —
/// and the name can. Discarding is the default posture and not a fault:
/// `trust_client_sampling` is off, so every client-supplied sampler value is
/// binned by design.
///
/// A proxy that reported no readback at all is **not** a proxy reporting an
/// empty tally, and the two must not print the same line: `gglib proxy
/// dashboard` is routinely pointed at a build older than this contract (see
/// [`super::wire_sampling`]), and rendering that silence as "nothing was
/// dropped" turns an unobserved state into a clean reading — the exact
/// collapse the section above this one exists to prevent.
pub(super) fn render_client_fields_section(audit: Option<&SamplingAudit>) -> String {
    let mut out = String::from("Client sampling dropped (trust_client_sampling off)\n");
    let Some(names) = audit.and_then(|a| a.client_field_names.as_ref()) else {
        out.push_str("  (not reported by this proxy)\n");
        return out;
    };

    if names.fields.is_empty() {
        out.push_str("  (no client sampling field has been dropped)\n");
        return out;
    }

    for tally in &names.fields {
        let mut what = Vec::new();
        if tally.discarded > 0 {
            what.push(format!("{} untrusted", thousands(tally.discarded)));
        }
        if tally.rejected > 0 {
            what.push(format!("{} unreadable", thousands(tally.rejected)));
        }
        out.push_str(&format!(
            "  {:<24} {}\n",
            truncate(&tally.field, 24),
            what.join(", ")
        ));
    }

    // Only when it fires: the tally is bounded, and a bound nobody can see is
    // indistinguishable from a bound nobody hit.
    if names.untracked > 0 {
        out.push_str(&format!(
            "  {:<24} {}\n",
            "(untracked names)",
            thousands(names.untracked)
        ));
    }
    out
}

#[cfg(test)]
#[path = "render_reasoning_tests.rs"]
mod tests;
