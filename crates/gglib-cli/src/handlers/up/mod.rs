#![doc = include_str!("README.md")]

mod choose;
pub(crate) mod client_block;
mod probe;
mod warm;

use std::ffi::OsStr;
use std::io::IsTerminal;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartProxyBody};
use crate::handlers::inference::proxy;

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
    /// Port the endpoint binds to; `None` leaves it to the daemon, which
    /// binds the stored `proxy_port`.
    pub port: Option<u16>,
}

/// What `up` asks the daemon to start: the unpinned endpoint as the stored
/// settings describe it, as the service's own launch starts it.
///
/// No host, so the stored `proxy_host` decides, loopback until one is stored:
/// `up` never changes where the endpoint listens. `port` is the `--port` flag
/// as typed; an absent flag travels as no port, so a stored `proxy_port` is
/// where the endpoint comes up and what the client configuration then prints.
fn start_body(port: Option<u16>, default_context: Option<u64>) -> StartProxyBody {
    StartProxyBody {
        host: None,
        port,
        default_context,
        // Unpinned: `/v1/models` has to work for Cline and Open WebUI to
        // discover anything. `up` warms one model; it does not restrict
        // to it. Everything else is deliberately unconfigurable here.
        ..Default::default()
    }
}

/// Execute the up command.
///
/// Returns once the endpoint has answered, saying what to run next: the
/// daemon owns the proxy, so it keeps serving after `up` exits.
pub(crate) async fn execute(ctx: &CliContext, args: UpArgs) -> Result<()> {
    println!();
    println!(
        "  {}gglib up{} \u{2014} from nothing to a working endpoint",
        sgr(crate::presentation::style::BOLD),
        sgr(crate::presentation::style::RESET)
    );

    // Steps 1-2: what this machine is, and llama.cpp on it.
    probe::run(ctx, args.yes).await?;

    // Step 3: the model. Either already here, or chosen, confirmed, downloaded.
    let model = choose::run(ctx, args.model.as_deref(), args.yes).await?;
    // A convenience: a settings write that fails is no reason to stop `up`.
    match choose::adopt_as_default(ctx, model.id).await {
        Ok(true) => row("default", &model.name, Some("gglib q uses it")),
        Ok(false) => {}
        Err(e) => tracing::warn!(error = %e, "could not make the served model the default"),
    }

    // Step 4: the proxy. Its own banners take over from here.
    step(4, "Endpoint");
    let settings = ctx.app.settings().get().await?;
    // Passed through, not resolved. `up` computes a fitted context and prints
    // it as the number that earns the user's trust; resolving here would turn
    // "the user set nothing" into "the user set 4096", which outranks the
    // fitted value at launch. Sending `None` lets the launch fit it.
    let default_context = settings.default_context_size;

    let handle = daemon_client::ensure_daemon_for_endpoint().await?;
    let body = start_body(args.port, default_context);
    // An endpoint that is on is working: `up` leaves one on otherwise as it
    // is, never asks to switch it, and `-y` does not switch it either.
    let started = endpoint_step(&handle, &body, &settings, args.port).await?;

    // Step 5: prove it. The daemon sends one real request through the very
    // endpoint the user is about to point their client at, by the id that
    // endpoint lists it under, and says how a client connects. A pinned
    // endpoint serves its own model, so that is the one proved.
    let proved = started.pinned_model_id.unwrap_or(model.id);
    warm::run(&handle, proved).await;
    for line in warm::render_next_steps(started.port, proved, use_color()) {
        println!("{line}");
    }
    Ok(())
}

/// Step 4's start: the endpoint as `body` asks, or the one that is on left
/// as it is, never switched, whatever `--yes` says; `port` is `--port`, and
/// names the command that would apply it.
async fn endpoint_step(
    handle: &daemon_client::DaemonHandle,
    body: &StartProxyBody,
    settings: &gglib_core::Settings,
    port: Option<u16>,
) -> Result<proxy::StartedProxy> {
    let apply = port.map_or_else(
        || "gglib proxy".to_owned(),
        |port| format!("gglib proxy --port {port}"),
    );
    proxy::start_or_leave(handle, body, settings, &apply).await
}

// ─── Shared output helpers ───────────────────────────────────────────────────

/// Whether to emit ANSI styling.
///
/// Same two conditions the launch narration honours (`NO_COLOR`, non-TTY
/// stdout), because the two blocks appear in the same output and disagreeing
/// about colour would be visible.
pub(crate) fn use_color() -> bool {
    color_allowed(
        std::env::var_os("NO_COLOR").as_deref(),
        std::io::stdout().is_terminal(),
    )
}

/// The rule [`use_color`] applies, given what `NO_COLOR` holds and whether
/// stdout is a terminal, so that a test can state it whatever terminal the
/// tests are run from. `NO_COLOR` set to anything, an empty value included,
/// turns colour off.
fn color_allowed(no_color: Option<&OsStr>, stdout_is_terminal: bool) -> bool {
    no_color.is_none() && stdout_is_terminal
}

/// An ANSI sequence, or nothing when colour is off.
///
/// Every escape this command emits goes through here. `up` prints from four
/// modules and the escapes are easy to sprinkle inline, which is exactly how a
/// piped run ends up with most of its output clean and one stray `\x1b[32m` in
/// the middle — the client configuration is the block most likely to be pasted
/// somewhere, so partial honouring of `NO_COLOR` is worse than none.
pub(super) fn sgr(code: &'static str) -> &'static str {
    sgr_when(use_color(), code)
}

/// [`sgr`], given whether colour is on.
fn sgr_when(color: bool, code: &'static str) -> &'static str {
    if color { code } else { "" }
}

/// Refuse to ask a question nobody is there to answer.
///
/// Closed stdin reads as EOF, which every confirmation takes for a no: the
/// user would be told llama.cpp is required, or that they cancelled a
/// download, over a question they never saw. That is not an answer. The flag
/// that *is* an answer is one word long, so name it.
pub(super) fn require_tty(action: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        crate::utils::input::Input::stdin() == crate::utils::input::Input::Terminal,
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
    vec![note.map_or_else(
        || format!("    {padded}  {value}"),
        |n| format!("    {padded}  {value}  {dim}({n}){reset}"),
    )]
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;
    use gglib_core::Settings;

    use super::*;
    use crate::commands::Commands;
    use crate::handlers::inference::proxy::tests::started;
    use crate::parser::Cli;

    /// The body `gglib up` sends for `argv`.
    fn body_for(argv: &[&str]) -> StartProxyBody {
        let Some(Commands::Up { port, .. }) = Cli::parse_from(argv).command else {
            panic!("{argv:?} is not `gglib up`");
        };
        start_body(port, None)
    }

    /// Without `--port` the daemon is sent no port, so the endpoint comes up
    /// on the stored `proxy_port`, where the desktop app would put it; and
    /// the port the client configuration prints is the one the daemon answers.
    #[tokio::test]
    async fn without_a_port_flag_the_daemon_chooses_the_port_and_its_answer_is_reported() {
        let body = body_for(&["gglib", "up"]);

        let started = started(&body, Some(9000), &Settings::default()).await;

        assert!(started.sent_no_port(), "{}", started.sent);
        assert_eq!(started.port, 9000);
    }

    /// No host is sent, so the stored `proxy_host` decides where the
    /// endpoint listens: `up` never rebinds one the settings put elsewhere.
    #[tokio::test]
    async fn the_stored_host_decides_where_the_endpoint_listens() {
        let body = body_for(&["gglib", "up"]);

        let started = started(&body, Some(9000), &Settings::default()).await;

        assert!(started.sent["host"].is_null(), "{}", started.sent);
    }

    #[tokio::test]
    async fn a_port_flag_is_sent_as_typed() {
        let body = body_for(&["gglib", "up", "--port", "8123"]);

        let started = started(&body, Some(8123), &Settings::default()).await;

        assert_eq!(started.sent["port"], 8123);
        assert_eq!(started.port, 8123);
    }

    /// `--model` names a model that has to be here, and one that is not is
    /// missed in the sentence every other command misses in.
    #[tokio::test]
    async fn a_requested_model_that_is_not_here_is_missed_as_everywhere_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = crate::bootstrap::test_context(dir.path()).await;

        let missed = choose::run(&ctx, Some("__no_such_model__"), true)
            .await
            .expect_err("no such model")
            .to_string();

        assert_eq!(
            missed,
            "No model found matching: '__no_such_model__'\n\
             Use 'gglib model list' to see available models."
        );
    }

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

    /// Colour needs both answers: `NO_COLOR` unset, and a terminal to draw on.
    #[test]
    fn colour_is_on_only_for_a_terminal_with_no_color_unset() {
        let set = |value: &'static str| Some(OsStr::new(value));

        assert!(color_allowed(None, true));
        assert!(!color_allowed(set("1"), true), "NO_COLOR is honoured");
        assert!(!color_allowed(set(""), true), "set to nothing is still set");
        assert!(!color_allowed(None, false), "a pipe gets no escapes");
        assert!(!color_allowed(set("1"), false));
    }

    /// The suppression path every inline escape in this command goes through.
    /// It regressed once: `row` honoured `NO_COLOR` while the `✓` markers
    /// printed raw green, so a redirected run came out almost-clean, which is
    /// the worst of both.
    #[test]
    fn sgr_emits_nothing_when_color_is_off() {
        use crate::presentation::style::{BOLD, SUCCESS};

        assert_eq!(sgr_when(false, SUCCESS), "");
        assert_eq!(sgr_when(false, BOLD), "");
        assert_eq!(sgr_when(true, SUCCESS), SUCCESS);
    }

    /// `sgr` is that path under this run's own answer. The tests may be run
    /// from a terminal or into a pipe, so the answer is not assumed here.
    #[test]
    fn sgr_follows_what_use_color_answers() {
        let code = crate::presentation::style::SUCCESS;
        assert_eq!(sgr(code), sgr_when(use_color(), code));
    }

    /// Where this run says colour must be off, `use_color` says so: with
    /// `NO_COLOR` set, or with stdout not a terminal, which is how CI runs
    /// these tests. From a terminal with `NO_COLOR` unset colour is on, and
    /// there is nothing here to hold it to.
    #[test]
    fn use_color_is_off_wherever_this_run_needs_it_off() {
        if std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal() {
            assert!(!use_color());
        }
    }

    /// Step 4 never switches an endpoint that is on otherwise, whatever
    /// `--yes` says: it sends no `replace`, reads the endpoint as it is, and
    /// says what it did not do with the command that would apply `--port`.
    #[tokio::test]
    async fn the_endpoint_step_leaves_an_endpoint_on_otherwise_as_it_is() {
        use crate::daemon_client::{paths, stand_in::against};

        let differs = serde_json::json!({
            "error": "The endpoint is on at port 8080, serving every model, so it was left as it is.",
            "type": paths::ENDPOINT_DIFFERS_TYPE,
            "metadata": {
                "now": "on at port 8080, serving every model",
                "action": "restart it with port 9000 (now 8080)",
                "moves": true,
            },
        });
        let status = serde_json::json!({
            "running": true, "state": "on", "port": 8080,
            "base_url": "http://127.0.0.1:8080/v1", "key": "none_needed",
            "pinned_model": null, "pinned_to": null, "pinned_model_id": null,
        });
        let body = body_for(&["gglib", "up", "--port", "9000", "-y"]);
        let (started, taken) = against(
            vec![(409, differs.to_string()), (200, status.to_string())],
            |handle| async move {
                endpoint_step(&handle, &body, &Settings::default(), Some(9000)).await
            },
        )
        .await;

        assert_eq!(started.expect("left as it is").port, 8080);
        assert_eq!(taken.len(), 2, "the start, then the status; no switch");
        assert_eq!(taken[0].body["replace"], false);
        assert_eq!(
            crate::handlers::inference::switch::left(
                &gglib_app_services::EndpointChange {
                    now: "on at port 8080, serving every model".into(),
                    action: "restart it with port 9000 (now 8080)".into(),
                    moves: true,
                },
                "gglib proxy --port 9000",
            ),
            [
                "  The endpoint is on at port 8080, serving every model, so gglib up left it as \
                 it is and did not restart it with port 9000 (now 8080).",
                "  To do that, run: gglib proxy --port 9000",
            ]
        );
    }
}
