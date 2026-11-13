//! Launch narration rendering.
//!
//! The narration assembled in `process::swap_state` is printed through here —
//! the banner is the proxy's output voice, but the launch it narrates happens
//! in the process manager. Free functions taking plain values, so the banner
//! has no opinion on where its inputs come from.

use std::io::IsTerminal;

use gglib_core::domain::{ContextFit, LaunchDecision, LaunchNarration};

const DIM: &str = "\u{1b}[2m";
/// ANSI bold, for the model identity line.
const BOLD: &str = "\u{1b}[1m";
const RESET: &str = "\u{1b}[0m";

/// Whether to emit ANSI styling on stdout.
///
/// Two independent reasons to decline, both of which must be honoured:
/// `NO_COLOR` (any value, per the convention) is an explicit request, and a
/// non-TTY stdout means the output is being piped or captured — a log file
/// full of escape sequences helps nobody.
fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// Print what the runtime decided for a launch, and why.
///
/// The visible counterpart to [`crate::launch_narration::narrate`]: gglib
/// auto-sizes the RAM cache, quantizes the KV cache, enables MTP and resolves
/// the context through a five-level chain, and before this existed it did all
/// of that in silence.
pub(crate) fn print_launch_narration(narration: &LaunchNarration) {
    for line in render_launch_narration(narration, use_color()) {
        println!("{line}");
    }
}

/// The rendered lines, as data.
///
/// Split from the printing so the layout is testable without capturing
/// stdout — the alignment and the provenance placement are the parts worth
/// asserting on, and neither is observable through `println!`.
fn render_launch_narration(narration: &LaunchNarration, color: bool) -> Vec<String> {
    let (dim, bold, reset) = if color {
        (DIM, BOLD, RESET)
    } else {
        ("", "", "")
    };

    let mut lines = vec![
        String::new(),
        format!("  {bold}{}{reset}", narration.headline()),
    ];

    // The fit's one row goes under the decisions: what the context row's
    // rung was snapped from, and from what. An image model has none.
    let fit = narration.fit.as_ref().map(ContextFit::banner_row);
    let rows: Vec<&LaunchDecision> = narration.decisions.iter().chain(fit.as_ref()).collect();

    // Pad labels to a common width so the values form a column. Computed from
    // the labels actually present rather than a constant, since which
    // decisions appear varies by launch.
    let width = rows.iter().map(|d| d.label.len()).max().unwrap_or(0);

    for decision in rows {
        let label = format!("{:<width$}", decision.label, width = width);
        let line = decision.source.as_ref().map_or_else(
            || format!("    {label}  {}", decision.value),
            |source| format!("    {label}  {}  {dim}({source}){reset}", decision.value),
        );
        lines.push(line);
    }

    lines.push(String::new());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::{FitInputs, KvBytes};
    use gglib_core::server_config::ContextSizeSource;

    fn narration() -> LaunchNarration {
        let mut n =
            LaunchNarration::new("qwen3-30b-a3b", Some("Q4_K_M".to_string()), 18_476_297_420);
        n.push(LaunchDecision::new("ctx", "32768", "model server_defaults"));
        n.push(LaunchDecision::new(
            "cache",
            "6.0 GiB",
            "auto from free RAM",
        ));
        n
    }

    /// The provenance in parentheses is the feature; without it the banner is
    /// just a config dump.
    #[test]
    fn every_sourced_decision_renders_its_provenance() {
        let lines = render_launch_narration(&narration(), false);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("32768") && l.contains("(model server_defaults)"))
        );
        assert!(lines.iter().any(|l| l.contains("(auto from free RAM)")));
    }

    #[test]
    fn headline_leads_the_block() {
        let lines = render_launch_narration(&narration(), false);
        assert_eq!(
            lines[1].trim(),
            "qwen3-30b-a3b \u{b7} Q4_K_M \u{b7} 17.2 GiB"
        );
    }

    /// Labels pad to a shared width so the values line up as a column.
    #[test]
    fn labels_are_padded_to_a_common_column() {
        let lines = render_launch_narration(&narration(), false);
        let ctx = lines.iter().find(|l| l.contains("32768")).unwrap();
        let cache = lines.iter().find(|l| l.contains("6.0 GiB")).unwrap();
        let ctx_value = ctx.find("32768").unwrap();
        let cache_value = cache.find("6.0 GiB").unwrap();
        assert_eq!(ctx_value, cache_value);
    }

    /// Piped or captured output must carry no escape sequences at all.
    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_launch_narration(&narration(), false);
        assert!(
            lines.iter().all(|l| !l.contains('\u{1b}')),
            "escape sequence leaked into uncoloured output"
        );
    }

    #[test]
    fn ansi_escapes_appear_only_when_color_is_on() {
        let lines = render_launch_narration(&narration(), true);
        assert!(lines.iter().any(|l| l.contains(DIM)));
        assert!(lines.iter().any(|l| l.contains(BOLD)));
    }

    /// The fit a launch was sized by: 48 GiB, 2.5 GB of weights and
    /// 1,097,000 bytes a token, which fit 40,005 tokens before 32K.
    fn sized(mut n: LaunchNarration) -> LaunchNarration {
        n.fit = Some(ContextFit::new(
            32_768,
            ContextSizeSource::FittedToHardware,
            Some(32_768),
            &FitInputs {
                budget_bytes: Some(48 << 30),
                weights_bytes: Some(2_500_000_000),
                kv: Some(KvBytes::uniform(1_097_000)),
                trained_ctx: Some(131_072),
                unsnapped: Some(40_005),
            },
        ));
        n
    }

    /// The fit is one row under the decisions, in their column, saying
    /// what the rung was snapped from and the figures it was fitted from.
    #[test]
    fn the_fit_is_one_row_under_the_decisions() {
        let lines = render_launch_narration(&sized(narration()), false);
        assert_eq!(
            lines[lines.len() - 2],
            "    fit    40005 before snapping  (48.00 GiB budget, 2.33 GiB weights, 1097000 bytes/token)"
        );
        assert_eq!(
            lines.len(),
            render_launch_narration(&narration(), false).len() + 1
        );
    }

    /// The mission's budget. Two blanks plus a headline plus the decisions,
    /// every decision a launch can make, and the fit's row.
    #[test]
    fn the_block_stays_under_twelve_lines() {
        let mut n = sized(narration());
        for label in ["backend", "runtime", "kv", "mtp", "flags", "sampling"] {
            n.push(LaunchDecision::new(label, "v", "s"));
        }
        let lines = render_launch_narration(&n, false);
        assert!(lines.len() <= 12, "{} lines is over budget", lines.len());
    }

    /// A narration with no decisions must not panic on the width computation.
    #[test]
    fn renders_a_bare_headline_without_decisions() {
        let n = LaunchNarration::new("solo", None, 0);
        let lines = render_launch_narration(&n, false);
        assert_eq!(lines[1].trim(), "solo");
    }
}
