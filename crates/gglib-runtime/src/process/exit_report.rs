//! What gglib's log says when a server it started stops (#2216), for
//! llama-server and `sd-server` alike.
//!
//! - A stop gglib asked for is one `info` line saying why ([`StopReason`]).
//! - An exit nobody asked for is one `warn` event: how it ended, and the
//!   last [`REPORTED_LINES`] lines it printed on either stream, each cut to
//!   [`REPORTED_LINE_CHARS`] characters.
//! - Except an exit by itself that is a stop and not a failure
//!   ([`is_a_stop`]): an exit code of 0, or a stop signal. That is one
//!   `info` line with the status. On macOS and Linux a server shares
//!   gglib's process group, so a terminal's Ctrl-C stops it and gglib at
//!   once, and gglib cannot tell that from a stop from elsewhere.
//!
//! Each server is told of once (`RunningProcess::told`), by whichever of
//! these sees its end first:
//!
//! - `GuiProcessCore::kill`, with the reason it was given; or, when the
//!   server had already exited before it was asked, as an exit by itself;
//! - `GuiProcessCore::kill_all_now`, as gglib shutting down;
//! - `GuiProcessCore::cleanup_dead`, as an exit by itself;
//! - [`watch`], as an exit by itself, for a server that stops while nothing
//!   is asking after it.

use std::fmt::Write as _;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};

use super::core::GuiProcessCore;
use super::output_tail::{OutputTail, SETTLE_WAIT};
use super::types::ServerInfo;

/// How many of the last lines a server printed an exit by itself reports.
pub(crate) const REPORTED_LINES: usize = 40;

/// How many characters of each reported line are kept.
pub(crate) const REPORTED_LINE_CHARS: usize = 1000;

/// How long [`watch`] looks for the exit status once a server's output has
/// ended: a process closes its streams as it exits, a moment before its
/// status can be read.
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// How often [`watch`] looks within [`EXIT_WAIT`].
const EXIT_TICK: Duration = Duration::from_millis(50);

/// SIGHUP, SIGINT and SIGTERM, the same numbers on macOS and Linux: a
/// closed terminal, a Ctrl-C, and a request to stop.
const STOP_SIGNALS: [i32; 3] = [1, 2, 15];

/// Why gglib stopped a server it started, as its log says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Somebody asked: a Stop, an unload, or a client's stop.
    Asked,
    /// Another launch took its slot, or the same model was asked for with
    /// other launch settings.
    Replaced,
    /// It failed its health check, or the proxy's watchdog found it
    /// degraded.
    Unhealthy,
    /// The launch that started it did not finish: it was not ready in time,
    /// or the launch was given up.
    LaunchAbandoned,
    /// The render it drew for ended.
    RenderEnded,
    /// gglib is shutting down.
    Shutdown,
}

impl StopReason {
    /// The reason, as the log line ends with it.
    const fn said(self) -> &'static str {
        match self {
            Self::Asked => "as it was asked to",
            Self::Replaced => "to make way for another launch",
            Self::Unhealthy => "as it was not answering as it should",
            Self::LaunchAbandoned => "as the launch that started it did not finish",
            Self::RenderEnded => "as its render ended",
            Self::Shutdown => "as gglib is shutting down",
        }
    }
}

/// Say in gglib's log that `info`'s server was stopped, and why.
pub(crate) fn stopped(info: &ServerInfo, why: StopReason) {
    info!(
        model_id = info.model_id,
        pid = info.pid,
        port = info.port,
        "{} for {} stopped, {}",
        info.runtime.server_name(),
        info.model_name,
        why.said()
    );
}

/// Say in gglib's log that `info`'s server exited by itself with `status`,
/// with the last lines in `output`.
///
/// The lines are read once its readers have reached the end of them (or
/// [`SETTLE_WAIT`] has passed), in a task of its own, so a caller holding
/// the process core's lock does not hold it for the wait. With no runtime
/// to run that task, they are read at once.
///
/// An exit that is a stop and not a failure ([`is_a_stop`]) is one `info`
/// line with its status, and no lines.
pub(crate) fn exited_by_itself(info: &ServerInfo, status: ExitStatus, output: OutputTail) {
    if is_a_stop(status.code(), signal_of(status)) {
        info!(
            model_id = info.model_id,
            pid = info.pid,
            port = info.port,
            "{} for {} exited by itself, with {}, which is not a failure",
            info.runtime.server_name(),
            info.model_name,
            ended_with(status)
        );
        return;
    }
    let info = info.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            output.settled(SETTLE_WAIT).await;
            write_exit(&info, status, &output.lines());
        });
    } else {
        write_exit(&info, status, &output.lines());
    }
}

/// The `warn` event for an exit by itself. It has no fields: a log line
/// gives an event's fields after its message, which would put them on the
/// end of the last line the server printed, so the message names the
/// server itself.
fn write_exit(info: &ServerInfo, status: ExitStatus, lines: &[String]) {
    warn!("{}", exit_said(info, status, lines));
}

/// The words for `info`'s server exiting by itself with `status`, having
/// printed `lines`: which server, how it ended, then its last
/// [`REPORTED_LINES`] lines, one to a line.
pub(crate) fn exit_said(info: &ServerInfo, status: ExitStatus, lines: &[String]) -> String {
    let mut said = format!(
        "{} for {} (model {}, pid {}, port {}) exited by itself, with {}",
        info.runtime.server_name(),
        info.model_name,
        info.model_id,
        info.pid,
        info.port,
        ended_with(status)
    );
    let tail = &lines[lines.len().saturating_sub(REPORTED_LINES)..];
    match tail.len() {
        0 => said.push_str("; it printed nothing"),
        1 => said.push_str(". The last line it printed:"),
        n => {
            let _ = write!(said, ". The last {n} lines it printed:");
        }
    }
    for line in tail {
        said.push_str("\n  | ");
        said.extend(line.chars().take(REPORTED_LINE_CHARS));
    }
    said
}

/// Whether a server that exited with `code`, or was ended by `signal`,
/// stopped rather than failed: a 0, which llama-server exits with once it
/// has handled a stop signal, or one of [`STOP_SIGNALS`].
pub(crate) fn is_a_stop(code: Option<i32>, signal: Option<i32>) -> bool {
    code == Some(0) || signal.is_some_and(|signal| STOP_SIGNALS.contains(&signal))
}

/// The signal that ended a process with `status`, if one did.
#[cfg(unix)]
fn signal_of(status: ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

/// Windows ends no process with a signal.
#[cfg(not(unix))]
fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}

/// How `status` ended, in the same words on every OS: its exit code, or,
/// with none (a signal, on Unix), the status as the OS says it.
fn ended_with(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| status.to_string(), |code| format!("exit code {code}"))
}

/// Watch the server `pid` that `GuiProcessCore::spawn` filed under
/// `model_id`, and tell gglib's log if it exits by itself.
///
/// Its end is seen when every stream of its `output` has been read to the
/// end, which a process's exit does, and its status is then looked for for
/// up to [`EXIT_WAIT`]. A server stopped through the core is no longer filed
/// under `model_id` by then, or has been told of, so this says nothing for
/// it. The core is held weakly, so a watch never keeps it, or its
/// `Drop`, from going.
pub(crate) fn watch(
    core: &Arc<RwLock<GuiProcessCore>>,
    model_id: u32,
    pid: u32,
    output: OutputTail,
) {
    let core = Arc::downgrade(core);
    tokio::spawn(async move {
        output.ended().await;
        let deadline = tokio::time::Instant::now() + EXIT_WAIT;
        loop {
            let Some(core) = core.upgrade() else { return };
            let running = core.write().await.tell_if_exited(model_id, pid);
            drop(core);
            if !running || tokio::time::Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(EXIT_TICK).await;
        }
    });
}

#[cfg(test)]
#[path = "exit_report_tests.rs"]
mod tests;
