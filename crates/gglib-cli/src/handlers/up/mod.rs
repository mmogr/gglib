#![doc = include_str!("README.md")]

mod choose;
mod probe;
mod warm;

use std::io::IsTerminal;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartProxyBody};

/// Loopback only. `up` is the "get me working" path; exposing an
/// unauthenticated endpoint to a network is a decision, and decisions belong to
/// `gglib proxy --host`.
const HOST: &str = "127.0.0.1";

/// Number of steps announced in the progress headers.
const STEPS: usize = 5;

/// Width the label column is padded to, matching the launch narration's
/// alignment so the two blocks read as one report.
const LABEL_WIDTH: usize = 8;

/// Parsed `gglib up` flags.
#[derive(Debug, Clone)]
pub(crate) struct UpArgs {
    /// Proceed with the model download without asking.
    pub yes: bool,
    /// Load this model rather than the recommended (or most recent) one.
    pub model: Option<String>,
    /// Port the endpoint binds to.
    pub port: u16,
}

/// Execute the up command.
///
/// Blocks until Ctrl-C, like `gglib proxy` — the endpoint it just built is the
/// point, so the command stays in the foreground serving it.
pub(crate) async fn execute(ctx: &CliContext, args: UpArgs) -> Result<()> {
    println!();
    println!(
        "  {}gglib up{} \u{2014} from nothing to a working endpoint",
        sgr(crate::presentation::style::BOLD),
        sgr(crate::presentation::style::RESET)
    );

    // Steps 1-2: what this machine is, and llama.cpp on it.
    let memory = probe::run(ctx, args.yes).await?;

    // Step 3: the model. Either already here, or chosen, confirmed, downloaded.
    let model = choose::run(ctx, &memory, args.model.as_deref(), args.yes).await?;

    // Step 4: the proxy. Its own banners take over from here.
    step(4, "Endpoint");
    let settings = ctx.app.settings().get().await?;
    // Passed through, not resolved. `up` is the path that computes a fitted
    // context, prints it as the number that earns the user's trust, and then
    // used to start the proxy at a flat 4096 — because resolving here turned
    // "the user set nothing" into "the user set 4096", which outranked the
    // fitted value at launch. Sending `None` lets the launch fit it.
    let default_context = settings.default_context_size;

    // The stored key, if any, is what the proxy about to start will demand of
    // this very probe: `up` binds loopback and passes no `--api-key`, so the
    // supervisor resolves the same settings row we read here.
    let api_key = settings
        .proxy_api_key
        .clone()
        .filter(|key| !key.trim().is_empty());

    let handle = daemon_client::ensure_daemon().await?;
    let status = handle
        .start_proxy(&StartProxyBody {
            host: Some(HOST.to_string()),
            port: Some(args.port),
            default_context,
            // Unpinned: `/v1/models` has to work for Cline and Open WebUI to
            // discover anything. `up` warms one model; it does not restrict
            // to it. Everything else is deliberately unconfigurable here.
            ..Default::default()
        })
        .await?;
    let proxy_port = status.port.unwrap_or(args.port);

    // Step 5: prove it. The warm request loads the model through the very
    // endpoint the user is about to point their client at.
    warm::run(proxy_port, model.name.clone(), api_key).await;

    // The endpoint now outlives this command — the daemon owns it. Attach the
    // dashboard so `up` keeps its "foreground until Ctrl-C" feel; detaching
    // leaves the endpoint serving.
    crate::handlers::inference::proxy::attach_dashboard(ctx, proxy_port, None).await
}

// ─── Shared output helpers ───────────────────────────────────────────────────

/// Whether to emit ANSI styling.
///
/// Same two conditions the launch narration honours (`NO_COLOR`, non-TTY
/// stdout), because the two blocks appear in the same output and disagreeing
/// about colour would be visible.
pub(super) fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// An ANSI sequence, or nothing when colour is off.
///
/// Every escape this command emits goes through here. `up` prints from four
/// modules and the escapes are easy to sprinkle inline, which is exactly how a
/// piped run ends up with most of its output clean and one stray `\x1b[32m` in
/// the middle — the client configuration is the block most likely to be pasted
/// somewhere, so partial honouring of `NO_COLOR` is worse than none.
pub(super) fn sgr(code: &'static str) -> &'static str {
    if use_color() { code } else { "" }
}

/// Refuse to ask a question nobody is there to answer.
///
/// Closed stdin reads as EOF, and both confirmation paths resolve EOF to their
/// default: `CliPrompt` defaults to yes, so `gglib up </dev/null` would kick
/// off a half-hour llama.cpp build unprompted, and `prompt_confirmation`
/// defaults to no, so the user would be told they cancelled a download they
/// never saw offered. Neither is an answer. The flag that *is* an answer is
/// one word long, so name it.
pub(super) fn require_tty(action: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal(),
        "Not running in a terminal, so there is nobody to confirm {action}.\n\
         Re-run with --yes to proceed without being asked."
    );
    Ok(())
}

/// Announce a step. The count is fixed, so a user can see how much is left
/// even when a step is skipped as already-done.
pub(super) fn step(n: usize, title: &str) {
    println!();
    println!(
        "{}[{n}/{STEPS}] {title}{}",
        sgr(crate::presentation::style::BOLD),
        sgr(crate::presentation::style::RESET)
    );
}

/// One `label  value  (note)` line, dimming the note.
///
/// Deliberately the same shape as `render_launch_narration` in
/// `gglib-runtime`: `up`'s own findings and the runtime's launch decisions
/// scroll past together, and a second layout would read as a second program.
pub(super) fn row(label: &str, value: &str, note: Option<&str>) {
    for line in render_row(label, value, note, use_color()) {
        println!("{line}");
    }
}

/// The rendered line, as data — split out so the layout is testable without
/// capturing stdout.
pub(super) fn render_row(label: &str, value: &str, note: Option<&str>, color: bool) -> Vec<String> {
    let (dim, reset) = if color {
        (
            crate::presentation::style::DIM,
            crate::presentation::style::RESET,
        )
    } else {
        ("", "")
    };
    let padded = format!("{label:<LABEL_WIDTH$}");
    vec![match note {
        Some(n) => format!("    {padded}  {value}  {dim}({n}){reset}"),
        None => format!("    {padded}  {value}"),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_renders_in_parentheses_after_the_value() {
        let lines = render_row("VRAM", "24.0 GiB", Some("nvidia-smi"), false);
        assert_eq!(lines[0], "    VRAM      24.0 GiB  (nvidia-smi)");
    }

    #[test]
    fn a_row_without_a_note_stops_after_the_value() {
        let lines = render_row("RAM", "64.0 GiB", None, false);
        assert_eq!(lines[0], "    RAM       64.0 GiB");
    }

    /// Values must form a column, as they do in the launch narration.
    #[test]
    fn labels_pad_to_a_common_width() {
        // A digit, so `find` cannot match a letter inside the label itself.
        let short = render_row("kv", "9", None, false).remove(0);
        let long = render_row("backend", "9", None, false).remove(0);
        assert_eq!(short.find('9'), long.find('9'));
    }

    #[test]
    fn no_ansi_escapes_when_color_is_off() {
        let lines = render_row("VRAM", "24.0 GiB", Some("probe"), false);
        assert!(!lines[0].contains('\u{1b}'));
    }

    /// Test stdout is never a TTY, so this pins the suppression path every
    /// inline escape in this command now goes through. It regressed once:
    /// `row` honoured `NO_COLOR` while the `✓` markers printed raw green, so a
    /// redirected run came out almost-clean, which is the worst of both.
    #[test]
    fn sgr_emits_nothing_when_color_is_off() {
        assert!(!use_color(), "test stdout should not be a terminal");
        assert_eq!(sgr(crate::presentation::style::SUCCESS), "");
        assert_eq!(sgr(crate::presentation::style::BOLD), "");
    }
}
