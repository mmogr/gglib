//! The last lines a server printed, kept for the launch waiting on it, and
//! the rule for the one line that says why a load failed (#1959 C5).
//!
//! The log manager keeps a server's output by port for the log view; a
//! port is used again by the next server, so a launch reads its own
//! server's lines from here instead.
//!
//! # The line that says why
//!
//! Of the last [`KEPT_LINES`] lines the server printed, on either stream,
//! blank lines skipped, each line is an error or not: when its engine marks
//! levels (llama.cpp's `0.00.125.192 E …`, stable-diffusion.cpp's `[E] …`),
//! one marked an error; when it marks none, one holding an error word
//! ([`ERROR_WORDS`], any case). The line that says why is then
//!
//! - the **first line of the last run of errors**: the last error line and
//!   the error lines before it, a run allowed one other line at a time
//!   between its errors ([`RUN_GAP`]); or
//! - the last line, when none is an error;
//!
//! with the level mark dropped and the line cut to [`SAID_CHARS`]
//! characters. It is the engine's own output: never the command line it was
//! started with.
//!
//! Why the run's first line, and not the last error line: a llama.cpp that
//! fails prints the cause and then each caller that gave up, ending on
//! `exiting due to model loading error` or `exiting due to HTTP server
//! error`, which is the last error line of every recorded failure that has
//! one (the recordings are in `output_tail_fixtures.rs`). Why the last run,
//! and not the first error line: an error printed early for something
//! optional (a probe, a cache) and followed by the server carrying on is
//! not the cause, and ordinary lines after it end its run before the fatal
//! one begins.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

/// How many lines are kept.
pub(crate) const KEPT_LINES: usize = 64;

/// How many characters of the line that says why are given.
pub(crate) const SAID_CHARS: usize = 300;

/// How many characters of any one line are kept: enough for a loader's
/// line listing every path it tried, which `failure_kind` reads whole.
const LINE_CHARS: usize = 4096;

/// How long a launch waits, once its server has exited, for the readers to
/// reach the end of its output. A program it started can hold a pipe open
/// past its exit, so this is bounded.
pub(crate) const SETTLE_WAIT: Duration = Duration::from_millis(500);

/// Words that mark a line as an error, in output with no level marks.
pub(crate) const ERROR_WORDS: &[&str] = &[
    "error",
    "failed",
    "fatal",
    "cannot",
    "can't",
    "couldn't",
    "could not",
    "unable",
    "invalid",
    "out of memory",
    "not found",
];

/// The last lines one server printed, shared by its readers and the launch.
#[derive(Debug, Clone)]
pub(crate) struct OutputTail {
    lines: Arc<Mutex<VecDeque<String>>>,
    /// How many of its streams are still being read.
    open: Arc<watch::Sender<usize>>,
}

impl Default for OutputTail {
    fn default() -> Self {
        Self {
            lines: Arc::default(),
            open: Arc::new(watch::channel(0).0),
        }
    }
}

impl OutputTail {
    /// A reader of one of its streams has started.
    pub(crate) fn opened(&self) {
        self.open.send_modify(|open| *open += 1);
    }

    /// A reader of one of its streams has reached its end.
    pub(crate) fn closed(&self) {
        self.open.send_modify(|open| *open = open.saturating_sub(1));
    }

    /// Keep `line`, made plain ([`plain`]), dropping the oldest past
    /// [`KEPT_LINES`]. The one place a line enters the tail, so nothing the
    /// engine printed reaches a sentence with a terminal's controls in it.
    pub(crate) fn push(&self, line: &str) {
        let line = plain(line, LINE_CHARS);
        let mut lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if lines.len() == KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// Wait until every stream has been read to its end, or `within` has
    /// passed.
    pub(crate) async fn settled(&self, within: Duration) {
        let mut open = self.open.subscribe();
        let _ = tokio::time::timeout(within, open.wait_for(|open| *open == 0)).await;
    }

    /// The lines kept, oldest first.
    pub(crate) fn lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// The line that says why, by the rule in the module docs.
    pub(crate) fn said(&self) -> Option<String> {
        telling_line(&self.lines())
    }
}

/// How a line's engine marked its level.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Marked an error.
    Error,
    /// Marked another level.
    Other,
    /// Not marked.
    None,
}

/// How many other lines in a row a run of errors may hold: llama.cpp prints
/// one, `cleaning up before exit...`, between its last two errors.
pub(crate) const RUN_GAP: usize = 1;

/// The line of `lines` that says why, by the rule in the module docs.
pub(crate) fn telling_line(lines: &[String]) -> Option<String> {
    let read: Vec<(Mark, &str)> = lines
        .iter()
        .map(|line| marked(line.trim()))
        .filter(|(_, text)| !text.is_empty())
        .collect();
    let last = read.last()?.1;
    let any_marked = read.iter().any(|(mark, _)| *mark != Mark::None);
    let is_error = |(mark, text): &(Mark, &str)| {
        if any_marked {
            *mark == Mark::Error
        } else {
            let text = text.to_lowercase();
            ERROR_WORDS.iter().any(|word| text.contains(word))
        }
    };
    let Some(end) = read.iter().rposition(is_error) else {
        return Some(cut(last));
    };
    // Back from the last error, over errors and gaps of at most `RUN_GAP`
    // other lines, to the first error of the run.
    let mut first = end;
    let mut gap = 0;
    for (i, line) in read[..end].iter().enumerate().rev() {
        if is_error(line) {
            first = i;
            gap = 0;
        } else {
            gap += 1;
            if gap > RUN_GAP {
                break;
            }
        }
    }
    Some(cut(read[first].1))
}

/// `line`'s level mark and the rest of it.
fn marked(line: &str) -> (Mark, &str) {
    // stable-diffusion.cpp: `[E] …`, `[ERROR] …`.
    if let Some(rest) = line.strip_prefix('[')
        && let Some((level, text)) = rest.split_once("] ")
        && matches!(
            level,
            "D" | "I" | "W" | "E" | "DEBUG" | "INFO" | "WARN" | "ERROR"
        )
    {
        let mark = if level.starts_with('E') {
            Mark::Error
        } else {
            Mark::Other
        };
        return (mark, text.trim());
    }
    // llama.cpp: `0.00.125.192 E …`, a timestamp then a one-letter level.
    let mut words = line.splitn(3, ' ');
    if let (Some(time), Some(level), Some(text)) = (words.next(), words.next(), words.next())
        && time.contains('.')
        && time.chars().all(|c| c.is_ascii_digit() || c == '.')
        && matches!(level, "D" | "I" | "W" | "E")
    {
        let mark = if level == "E" {
            Mark::Error
        } else {
            Mark::Other
        };
        return (mark, text.trim());
    }
    (Mark::None, line)
}

/// `line`, cut to [`SAID_CHARS`] characters.
fn cut(line: &str) -> String {
    if line.chars().count() <= SAID_CHARS {
        return line.to_owned();
    }
    let mut kept: String = line.chars().take(SAID_CHARS - 1).collect();
    kept.push('…');
    kept
}

/// One line of an engine's output as a sentence may give it: [`plain`],
/// trimmed, and cut to [`SAID_CHARS`] characters. For a line read outside a
/// tail, such as the first an engine wrote when asked `--version`.
pub(crate) fn said_line(line: &str) -> String {
    cut(plain(line, LINE_CHARS).trim())
}

/// `line` with no terminal control in it, at most `max` characters: every
/// ANSI escape sequence (CSI such as colours and `ESC[2J`, OSC such as a
/// title or `ESC]52` clipboard write, and the other `ESC` sequences) taken
/// out whole, a tab made a space, and every other control character (`\r`
/// included) dropped. What an engine prints can carry a string from a
/// model file, so none of it reaches a terminal as a control.
pub(crate) fn plain(line: &str, max: usize) -> String {
    let mut chars = line.chars();
    let mut out = String::new();
    let mut kept = 0;
    while kept < max {
        let Some(c) = chars.next() else { break };
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            '\u{9b}' => skip_csi(&mut chars),
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars),
            '\t' => {
                out.push(' ');
                kept += 1;
            }
            c if c.is_control() => {}
            c => {
                out.push(c);
                kept += 1;
            }
        }
    }
    out
}

/// The rest of an escape sequence, after its `ESC`.
fn skip_escape(chars: &mut impl Iterator<Item = char>) {
    match chars.next() {
        Some('[') => skip_csi(chars),
        // Strings: OSC, DCS, SOS, PM, APC.
        Some(']' | 'P' | 'X' | '^' | '_') => skip_string(chars),
        // A character set or the like: one more character.
        Some('(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%' | ' ') => {
            chars.next();
        }
        // Any other is two characters long.
        _ => {}
    }
}

/// A CSI sequence's parameters, up to and including its final byte.
fn skip_csi(chars: &mut impl Iterator<Item = char>) {
    for c in chars.by_ref() {
        if ('@'..='~').contains(&c) {
            return;
        }
    }
}

/// A control string, up to and including its terminator: BEL, or ST
/// (`ESC \` or the one-character form).
fn skip_string(chars: &mut impl Iterator<Item = char>) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' | '\u{9c}' => return,
            '\u{1b}' => {
                chars.next();
                return;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "output_tail_tests.rs"]
mod tests;
