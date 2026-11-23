//! One line on stderr that each new report rewrites: a long tool's
//! progress, a wait before a reply, or a wait on Hugging Face's rate limit.
//!
//! On a terminal the line is redrawn in place and wiped when what it reports
//! is over; written to a pipe, each report is its own line, and a wait on the
//! Hub's limit is said once, as it starts, rather than once a second.

use std::io::{self, IsTerminal as _, Write as _};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use gglib_core::hub_limit::LimitNotice;
use indicatif::{ProgressBar, ProgressStyle};

use crate::console::CliConsole;
use crate::presentation::style::{DIM, RESET};

/// Wipes the terminal line the cursor is on and returns to its start.
const WIPE: &str = "\r\x1b[2K";

/// Write `line`, rewriting the last one on a terminal.
pub(crate) fn show(line: &str) {
    let mut err = io::stderr();
    if err.is_terminal() {
        let _ = write!(err, "{WIPE}  {DIM}{line}{RESET}");
    } else {
        let _ = writeln!(err, "  {line}");
    }
    let _ = err.flush();
}

/// Wipe a status line, on a terminal, before what follows it is written.
pub(crate) fn wipe() {
    let mut err = io::stderr();
    if err.is_terminal() {
        let _ = write!(err, "{WIPE}");
        let _ = err.flush();
    }
}

/// The notice a one-shot command's Hub client is told a rate-limit wait
/// through.
///
/// On a terminal the sentence is one line among `console`'s bars, counting
/// down and taken away when the wait is over, so a wait while a download's
/// bars are drawn (as `gglib up`'s can be) is drawn with them rather than
/// over them. On a pipe it is [`show`]n once a wait, as the wait starts.
pub(crate) fn hub_wait_notice(console: Arc<CliConsole>) -> LimitNotice {
    let said = AtomicBool::new(false);
    let line: Mutex<Option<ProgressBar>> = Mutex::new(None);
    Arc::new(move |told: Option<&str>| {
        if !console.draws_bars() {
            if first_of_its_wait(&said, told.is_some())
                && let Some(told) = told
            {
                show(told);
            }
            return;
        }
        let mut line = line.lock().unwrap_or_else(PoisonError::into_inner);
        match told {
            Some(told) => line
                .get_or_insert_with(|| console.add_bar(waiting_line()))
                .set_message(format!("{DIM}{told}{RESET}")),
            None => {
                if let Some(bar) = line.take() {
                    console.remove_bar(&bar);
                }
            }
        }
    })
}

/// The bar a wait's sentence is drawn as: its message alone, indented as
/// [`show`] indents a line.
fn waiting_line() -> ProgressBar {
    let style =
        ProgressStyle::with_template("  {msg}").unwrap_or_else(|_| ProgressStyle::default_bar());
    ProgressBar::new_spinner().with_style(style)
}

/// Whether a notice that a wait goes on (`going_on`) is the first of its
/// wait, given `said`, whether this wait has been said already; a notice
/// that it is over makes the next one the first of a new wait.
fn first_of_its_wait(said: &AtomicBool, going_on: bool) -> bool {
    if going_on {
        !said.swap(true, Ordering::SeqCst)
    } else {
        said.store(false, Ordering::SeqCst);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which of a pipe's notices are written, in order.
    fn written_to_a_pipe(told: &[Option<&str>]) -> Vec<String> {
        let said = AtomicBool::new(false);
        told.iter()
            .filter(|line| first_of_its_wait(&said, line.is_some()))
            .filter_map(|line| line.map(str::to_owned))
            .collect()
    }

    /// Two waits, each told twice and then ended.
    const TWO_WAITS: [Option<&str>; 6] = [
        Some("waiting for Hugging Face's rate limit, 0:02"),
        Some("waiting for Hugging Face's rate limit, 0:01"),
        None,
        Some("waiting for Hugging Face's rate limit, 1:00"),
        Some("waiting for Hugging Face's rate limit, 0:59"),
        None,
    ];

    #[test]
    fn a_pipe_is_told_each_wait_once_as_it_starts() {
        assert_eq!(
            written_to_a_pipe(&TWO_WAITS),
            [
                "waiting for Hugging Face's rate limit, 0:02",
                "waiting for Hugging Face's rate limit, 1:00",
            ]
        );
    }

    /// On a terminal the sentence is drawn as one of the console's lines.
    #[test]
    fn a_terminal_draws_the_wait_among_the_consoles_lines() {
        let (console, term) = CliConsole::on_unseen_term();
        let notice = hub_wait_notice(Arc::new(console));

        notice(Some("waiting for Hugging Face's rate limit, 0:02"));
        notice(None);

        let written = term.written();
        assert!(
            written
                .iter()
                .any(|line| line.contains("waiting for Hugging Face's rate limit, 0:02")),
            "{written:?}"
        );
    }
}
