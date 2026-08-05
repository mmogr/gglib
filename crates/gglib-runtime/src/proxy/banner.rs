//! Startup banners for [`start_proxy_standalone`](super::start_proxy_standalone).
//!
//! Split out purely for size: `start_proxy_standalone` is the sequencing
//! (config → manager → supervisor), and this ~90-line block of `println!`
//! calls was by far its largest single piece. Free functions taking plain
//! values rather than the runtime's own types, so the banner has no opinion
//! on where its inputs come from and stays trivial to read top to bottom.

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::Path;

use gglib_core::domain::{InferenceConfig, LaunchNarration};

use super::params::PinnedModel;

/// Printed once, before the supervisor binds — everything known ahead of the
/// actual listen address.
#[allow(clippy::too_many_arguments)]
pub(super) fn print_starting(
    pinned: Option<&PinnedModel>,
    host: &str,
    port: u16,
    llama_base_port: u16,
    default_context: u64,
    inference_override: Option<&InferenceConfig>,
    cache_enabled: bool,
    resolved_slot_dir: Option<&Path>,
    mcp_server_count: usize,
    mcp_eager_count: usize,
    mcp_lazy_count: usize,
    mcp_manual_count: usize,
    mcp_tool_count: usize,
) {
    println!();
    match pinned {
        Some(_) => println!("  🚀 gglib serve starting (pinned)..."),
        None => println!("  🚀 gglib proxy starting..."),
    }
    println!();
    println!("  Host:            {host}");
    println!("  Port:            {port}");
    println!("  Llama base port: {llama_base_port}");
    println!("  Default context: {default_context}");
    if let Some(model) = pinned {
        // Stated up front because it changes what the endpoint will accept:
        // every other model is refused rather than swapped in.
        println!(
            "  Pinned model:    {} (id {}) — other models will be refused",
            model.name, model.id
        );
    }
    if let Some(ic) = inference_override {
        println!("  Inference override: {}", format_inference_override(ic));
    }
    print_cache_state(cache_enabled, resolved_slot_dir);
    println!(
        "  MCP servers:     {mcp_server_count} (eager: {mcp_eager_count}, lazy: {mcp_lazy_count}, manual: {mcp_manual_count})"
    );
    println!("  MCP tools:       {mcp_tool_count} (eager-started)");
    println!();
}

/// Render the sampling overrides a caller supplied on the command line.
///
/// Only the fields actually set are listed — an all-`None` config never
/// reaches here, since [`Option::is_some`] gates the call at the print site.
fn format_inference_override(ic: &InferenceConfig) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = ic.temperature {
        parts.push(format!("temperature={v}"));
    }
    if let Some(v) = ic.top_p {
        parts.push(format!("top_p={v}"));
    }
    if let Some(v) = ic.top_k {
        parts.push(format!("top_k={v}"));
    }
    if let Some(v) = ic.max_tokens {
        parts.push(format!("max_tokens={v}"));
    }
    if let Some(v) = ic.repeat_penalty {
        parts.push(format!("repeat_penalty={v}"));
    }
    if let Some(v) = ic.presence_penalty {
        parts.push(format!("presence_penalty={v}"));
    }
    if let Some(v) = ic.min_p {
        parts.push(format!("min_p={v}"));
    }
    parts.join(", ")
}

/// A `slots/` directory appears on disk the moment caching is on — worth
/// stating up front rather than letting a user discover it by accident,
/// especially in a source checkout where the default resolves inside the
/// repo itself.
fn print_cache_state(cache_enabled: bool, resolved_slot_dir: Option<&Path>) {
    match (cache_enabled, resolved_slot_dir) {
        (true, Some(dir)) => println!("  KV slot cache:   enabled ({})", dir.display()),
        (true, None) => println!("  KV slot cache:   enabled"),
        (false, _) => println!("  KV slot cache:   disabled (--cache to enable)"),
    }
}

/// Printed once the supervisor has actually bound — the pieces that depend
/// on the real listen address.
///
/// Framing is mode-aware: `gglib serve` exists for clients that *cannot*
/// switch models via `/v1/models`, so "Configure OpenWebUI" — a client that
/// can — is the wrong invitation for a pinned endpoint.
pub(super) fn print_ready(addr: SocketAddr, pinned: Option<&PinnedModel>) {
    println!("  ✓ Proxy started successfully on {addr}");
    println!();
    if pinned.is_some() {
        println!("  Point your OpenAI-compatible client at:");
    } else {
        println!("  Configure OpenWebUI:");
    }
    println!("    OpenAI API: http://{addr}/v1");
    println!("    MCP Tools:  http://{addr}/mcp");
    println!("    Dashboard:  http://{addr}/v1/proxy/status");
    println!();
    println!("  Press Ctrl+C to stop");
    println!();
}

// =============================================================================
// Launch narration
// =============================================================================

/// ANSI dim, for the provenance in parentheses.
///
/// Only the provenance is styled: the values are the content, and dimming the
/// reasons is what lets the eye read the column of decisions first and the
/// explanations second.
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

    /// Not a snapshot test — `println!` output isn't worth pinning byte for
    /// byte — but `format_inference_override` is the one piece with real
    /// branching (which fields print, in what order), so it gets a direct
    /// assertion rather than relying on someone eyeballing terminal output.
    #[test]
    fn format_inference_override_lists_only_set_fields_in_declared_order() {
        let ic = InferenceConfig {
            temperature: Some(0.7),
            min_p: Some(0.05),
            ..Default::default()
        };
        assert_eq!(
            format_inference_override(&ic),
            "temperature=0.7, min_p=0.05"
        );
    }

    #[test]
    fn format_inference_override_empty_config_yields_empty_string() {
        assert_eq!(format_inference_override(&InferenceConfig::default()), "");
    }
}
