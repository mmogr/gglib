//! The download board: the queue's rows, drawn on the terminal.
//!
//! One line per download, made from the row's own text, so the CLI prints
//! what the GUI does, less the one word a plain transfer has for a status.
//! On a terminal each row is a progress bar, redrawn in place. Piped, each
//! row is a plain line, printed again every two seconds.
//!
//! The board draws whatever snapshot it is handed: the daemon's, polled over
//! HTTP, or this process's own download manager's.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};

use gglib_core::download::{
    DownloadOutcome, DownloadRow, FinishedDownload, QueueSnapshot, STATUS_DOWNLOADING,
};

use crate::console::CliConsole;

/// The bar is 30 cells, and the row's words take the rest of the line.
///
/// No `{bytes_per_sec}` or `{eta}`: those are indicatif's own estimates,
/// derived from the positions it is given. The speed and the time remaining
/// are the download manager's, and arrive as text on the row.
const TEMPLATE: &str = "{prefix} [{bar:30}] {wide_msg}";

/// A bar's length. Its position is the row's percentage in tenths, rounded
/// down.
const BAR_LENGTH: u64 = 1000;

/// How often the rows are printed when they cannot be redrawn in place.
const PLAIN_EVERY: Duration = Duration::from_secs(2);

/// What a row puts on its bar.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BarParts {
    /// The download's name and the file it is on, before the bar.
    pub prefix: String,
    /// How full the bar is, of [`BAR_LENGTH`]. Empty while the size is not
    /// known.
    pub position: u64,
    /// What it is doing, after the bar.
    pub message: String,
}

/// The parts of `row`'s bar, all from the row's own text.
pub(crate) fn bar_parts(row: &DownloadRow) -> BarParts {
    let text = &row.text;
    let prefix = text.file.as_ref().map_or_else(
        || text.title.clone(),
        |file| format!("{} · {file}", text.title),
    );

    // A transfer that is going well needs no word for it: its numbers say
    // so. Any other status leads, a note standing in for progress included.
    // This is the one thing the line leaves out of the row's text.
    let status = if text.status == STATUS_DOWNLOADING {
        ""
    } else {
        &text.status
    };
    let message = [status, &text.percent, &text.bytes, &text.speed, &text.eta]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");

    // The fill is the row's own percentage, which is what the GUI's bar is
    // drawn from. The row keeps it between 0 and 100.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a percentage from 0 to 100, in tenths"
    )]
    let position = row
        .percent
        .map_or(0, |percent| (percent * 10.0).floor() as u64);

    BarParts {
        prefix,
        position,
        message,
    }
}

/// `row` as one line of plain text, for output that is not a terminal.
pub(crate) fn plain_line(row: &DownloadRow) -> String {
    let parts = bar_parts(row);
    format!("{}  {}", parts.prefix, parts.message)
}

/// How a download ended, as the line printed for it: a mark for whether
/// it arrived, and the entry's own words.
pub(crate) fn outcome_line(ended: &FinishedDownload) -> String {
    let mark = match ended.outcome {
        DownloadOutcome::Completed { .. } => '✓',
        DownloadOutcome::Failed { .. } | DownloadOutcome::Cancelled => '✗',
    };
    format!("{mark} {}", ended.text)
}

/// The queue's rows on the terminal.
pub(crate) struct DownloadBoard {
    console: Arc<CliConsole>,
    /// One bar per row, by the row's id.
    bars: HashMap<String, ProgressBar>,
    /// When the rows were last printed as plain lines.
    printed: Option<Instant>,
}

impl DownloadBoard {
    /// A board on `console`, with nothing on it yet.
    pub(crate) fn new(console: Arc<CliConsole>) -> Self {
        Self {
            console,
            bars: HashMap::new(),
            printed: None,
        }
    }

    /// Show `snapshot`: a line for each of its rows, and none for a row
    /// that has gone.
    pub(crate) fn sync(&mut self, snapshot: &QueueSnapshot) {
        if self.console.draws_bars() {
            self.sync_bars(snapshot);
        } else if self
            .printed
            .is_none_or(|printed| printed.elapsed() >= PLAIN_EVERY)
            && !snapshot.is_idle()
        {
            for row in snapshot.rows() {
                self.console.println(&plain_line(row));
            }
            self.printed = Some(Instant::now());
        }
    }

    /// Keep one bar per row, keyed by the row's id: a download keeps its bar
    /// from its first file to its last.
    fn sync_bars(&mut self, snapshot: &QueueSnapshot) {
        for row in snapshot.rows() {
            let parts = bar_parts(row);
            let bar = self.bars.entry(row.id.clone()).or_insert_with(|| {
                let style = ProgressStyle::with_template(TEMPLATE)
                    .unwrap_or_else(|_| ProgressStyle::default_bar())
                    .progress_chars("=> ");
                self.console
                    .add_bar(ProgressBar::new(BAR_LENGTH).with_style(style))
            });
            bar.set_prefix(parts.prefix);
            bar.set_position(parts.position);
            bar.set_message(parts.message);
        }

        let console = &self.console;
        self.bars.retain(|id, bar| {
            let live = snapshot.rows().any(|row| &row.id == id);
            if !live {
                console.remove_bar(bar);
            }
            live
        });
    }

    /// Take every bar off the screen.
    pub(crate) fn clear(&mut self) {
        for (_, bar) in self.bars.drain() {
            self.console.remove_bar(&bar);
        }
    }

    /// Print how each of `ended` ended, one line apiece.
    pub(crate) fn report(&self, ended: &[FinishedDownload]) {
        for ended in ended {
            self.console.println(&outcome_line(ended));
        }
    }
}

#[cfg(test)]
#[path = "board_tests.rs"]
mod tests;
