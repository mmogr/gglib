//! Launch narration rendering.
//!
//! The narration assembled in `process::swap_state` is printed through here —
//! the banner is the proxy's output voice, but the launch it narrates happens
//! in the process manager. Free functions taking plain values, so the banner
//! has no opinion on where its inputs come from.

use std::io::IsTerminal;

use gglib_core::domain::LaunchNarration;

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
/// auto-sizes the RAM cache, quantizes the KV cache, enables MTP, picks a
/// dialect parser and resolves the context through a four-level chain, and
/// before this existed it did all of that in silence.
pub fn print_launch_narration(narration: &LaunchNarration) {
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

    // Pad labels to a common width so the values form a column. Computed from
    // the labels actually present rather than a constant, since which
    // decisions appear varies by launch.
    let width = narration
        .decisions
        .iter()
        .map(|d| d.label.len())
        .max()
        .unwrap_or(0);

    for decision in &narration.decisions {
        let label = format!("{:<width$}", decision.label, width = width);
        let line = match &decision.source {
            Some(source) => format!("    {label}  {}  {dim}({source}){reset}", decision.value),
            None => format!("    {label}  {}", decision.value),
        };
        lines.push(line);
    }

    lines.push(String::new());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::LaunchDecision;

    fn narration() -> LaunchNarration {
        let mut n =
            LaunchNarration::new("qwen3-30b-a3b", Some("Q4_K_M".to_string()), 18_476_297_420);
        n.push(LaunchDecision::new("ctx", "32768", "model server_defaults"));
        n.push(LaunchDecision::new(
            "dialect",
            "qwen-xml -> OpenAI tool_calls",
            "format:qwen-xml tag",
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
        assert!(lines.iter().any(|l| l.contains("(format:qwen-xml tag)")));
    }

    #[test]
    fn headline_leads_the_block() {
        let lines = render_launch_narration(&narration(), false);
        assert_eq!(
            lines[1].trim(),
            "qwen3-30b-a3b \u{b7} Q4_K_M \u{b7} 17.2 GB"
        );
    }

    /// Labels pad to a shared width so the values line up as a column.
    #[test]
    fn labels_are_padded_to_a_common_column() {
        let lines = render_launch_narration(&narration(), false);
        let ctx = lines.iter().find(|l| l.contains("32768")).unwrap();
        let dialect = lines.iter().find(|l| l.contains("qwen-xml")).unwrap();
        let ctx_value = ctx.find("32768").unwrap();
        let dialect_value = dialect.find("qwen-xml").unwrap();
        assert_eq!(ctx_value, dialect_value);
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

    /// The mission's budget. Two blanks plus a headline plus the decisions.
    #[test]
    fn the_block_stays_under_twelve_lines() {
        let mut n = narration();
        for label in ["backend", "kv", "cache", "mtp", "flags"] {
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
