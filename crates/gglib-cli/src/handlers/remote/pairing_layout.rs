//! What the pairing screen shows in a window of a given size, and how it is
//! put there.
//!
//! Two rules, and both are about the scrollback. A frame never has more rows
//! than the window it was laid out for, so painting it scrolls nothing; and
//! nothing here erases the display, only single lines. iTerm2 keeps what
//! scrolls off the alternate screen, and keeps the whole screen each time it
//! is erased, so a frame that did either once a second left a copy of the
//! pairing string behind once a second.
//!
//! One gap is left open. A window made smaller between the read of its size
//! and the paint gets a frame laid out for the larger one, which can scroll
//! once; the next paint fits it again.

use std::io::{self, Write};

use crossterm::style::Print;
use crossterm::terminal::{Clear, ClearType, DisableLineWrap, EnableLineWrap};
use crossterm::{cursor, queue, terminal};
use gglib_app_services::RemoteEnableResponse;

/// The window assumed when the terminal will not say, or says nothing has
/// any room.
const FALLBACK: (u16, u16) = (80, 24);

/// What a window too small for anything else is told.
const TOO_SMALL: &str = "  Window too small. Enlarge it, or Ctrl-C and rerun with --no-qr.";

/// One line of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) text: String,
    /// Printed with line wrap on, over as many rows as it takes. Only the
    /// join command is: cut short it is useless. Any other line too long for
    /// the window is cut at its edge by the terminal.
    pub(super) wrap: bool,
}

impl Line {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            wrap: false,
        }
    }

    /// The rows this line takes in a window `cols` wide.
    ///
    /// A wrapped line that ends exactly at the edge is counted a row over,
    /// rather than trusting the terminal to hold the cursor there.
    pub(super) fn rows(&self, cols: u16) -> usize {
        if self.wrap {
            self.text.len() / usize::from(cols.max(1)) + 1
        } else {
            1
        }
    }
}

/// The lines for one window size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    /// Columns, then rows.
    pub(super) size: (u16, u16),
    pub(super) lines: Vec<Line>,
}

impl Frame {
    /// The screen for `offer` in a window of `size`, with `left_s` seconds
    /// before the code expires.
    pub(super) fn lay(
        size: (u16, u16),
        offer: &RemoteEnableResponse,
        qr: Option<&str>,
        left_s: u64,
    ) -> Self {
        Self {
            size,
            lines: layout(size, offer, qr, left_s),
        }
    }
}

/// The window's size as the terminal gives it, or [`FALLBACK`] when it gives
/// an error or a dimension of zero.
pub(super) fn usable(size: io::Result<(u16, u16)>) -> (u16, u16) {
    size.ok()
        .filter(|&(cols, rows)| cols > 0 && rows > 0)
        .unwrap_or(FALLBACK)
}

/// `text` with every control character replaced, so that nothing the daemon
/// sent can move the cursor or switch a terminal mode.
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// The join command, whole, or `None` for a pairing that is not one.
///
/// Indented like the rest when it fits the width; otherwise from the first
/// column and wrapped, where a terminal copies it back out as one line.
fn join_line(pairing: &str, cols: u16) -> Option<Line> {
    if pairing.is_empty() || !pairing.bytes().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    let indented = format!("    gglib remote join {pairing}");
    Some(if indented.len() < usize::from(cols) {
        Line::plain(indented)
    } else {
        Line {
            text: format!("gglib remote join {pairing}"),
            wrap: true,
        }
    })
}

/// The most that fits: the first of a fixed list of frames, fullest first,
/// that is no taller than the window.
///
/// The QR outranks the text. A phone has no other way in, and the join
/// command has `--no-qr`.
fn layout(
    (cols, rows): (u16, u16),
    offer: &RemoteEnableResponse,
    qr: Option<&str>,
    left_s: u64,
) -> Vec<Line> {
    let blank = || Line::plain("");
    let join = join_line(offer.pairing.as_deref().unwrap_or_default(), cols);
    let code = Line::plain(format!(
        "  code    {}",
        clean(offer.code.as_deref().unwrap_or_default())
    ));
    let countdown = Line::plain(format!(
        "  {left_s}s left. Waiting for a device\u{2026} Ctrl-C leaves the tunnel up."
    ));
    let drawn: Vec<Line> = qr
        .into_iter()
        .flat_map(str::lines)
        .map(|row| Line::plain(format!("  {row}")))
        .collect();
    // In characters: a block character is three bytes and one column.
    let qr_cols = drawn
        .iter()
        .map(|l| l.text.chars().count())
        .max()
        .unwrap_or(0);
    let qr_fits =
        !drawn.is_empty() && qr_cols <= usize::from(cols) && drawn.len() <= usize::from(rows);

    let mut frames: Vec<Vec<Line>> = Vec::new();
    if qr_fits {
        if let Some(join) = &join {
            let mut full = vec![
                Line::plain("  gglib remote \u{2014} pair a device"),
                blank(),
            ];
            full.extend(drawn.iter().cloned());
            full.extend([
                blank(),
                Line::plain("  On the other machine:"),
                blank(),
                join.clone(),
                blank(),
            ]);
            // Cut at the window's edge a ticket would read as a whole one
            // that is wrong, and the join command carries it anyway.
            let ticket = format!("  ticket  {}", clean(&offer.ticket));
            if ticket.chars().count() < usize::from(cols) {
                full.push(Line::plain(ticket));
            }
            full.push(code);
            if let Some(device) = &offer.device {
                full.push(Line::plain(format!("  device  {}", clean(device))));
            }
            full.extend([blank(), countdown.clone()]);
            frames.push(full);
            frames.push([&drawn[..], &[join.clone(), countdown.clone()]].concat());
        }
        frames.push([&drawn[..], &[countdown]].concat());
        frames.push(drawn);
    } else if let Some(join) = join {
        let mut text = Vec::new();
        if !drawn.is_empty() {
            text.push(Line::plain(format!(
                "  QR hidden: it needs a window {qr_cols} wide and {} tall.",
                drawn.len()
            )));
        }
        text.extend([join.clone(), code, countdown.clone()]);
        frames.push(text);
        frames.push(vec![join, countdown]);
    }
    frames
        .into_iter()
        .find(|frame| frame.iter().map(|l| l.rows(cols)).sum::<usize>() <= usize::from(rows))
        .unwrap_or_else(|| vec![Line::plain(TOO_SMALL)])
}

/// Put `frame` on the screen, given what `shown` put there before.
///
/// A frame the same shape as the last has only its changed lines rewritten,
/// which is the countdown: the rest stays as it was, selection included, so
/// the join command can be copied. Line wrap is off except while the join
/// command is printed, so a line too long for the window is cut by the
/// terminal and cannot push the screen up; it is back on when this returns `Ok`.
pub(super) fn paint(out: &mut impl Write, frame: &Frame, shown: Option<&Frame>) -> io::Result<()> {
    let (cols, rows) = frame.size;
    let shown = shown.filter(|shown| {
        shown.size == frame.size
            && shown.lines.len() == frame.lines.len()
            && shown
                .lines
                .iter()
                .zip(&frame.lines)
                .all(|(was, is)| was.wrap == is.wrap && (!is.wrap || was == is))
    });
    queue!(out, DisableLineWrap)?;
    if shown.is_none() {
        for row in 0..rows {
            queue!(out, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        }
    }
    let mut row = 0;
    for (at, line) in frame.lines.iter().enumerate() {
        if shown.is_none_or(|shown| shown.lines[at] != *line) {
            queue!(
                out,
                cursor::MoveTo(0, u16::try_from(row).unwrap_or(u16::MAX))
            )?;
            if line.wrap {
                queue!(out, EnableLineWrap, Print(&line.text), DisableLineWrap)?;
            } else {
                queue!(out, Clear(ClearType::CurrentLine), Print(&line.text))?;
            }
        }
        row += line.rows(cols);
    }
    queue!(out, EnableLineWrap)?;
    out.flush()
}

/// Give the terminal back: line wrap on, the cursor shown, the alternate
/// screen left.
pub(super) fn restore(out: &mut impl Write) -> io::Result<()> {
    queue!(
        out,
        EnableLineWrap,
        cursor::Show,
        terminal::LeaveAlternateScreen
    )?;
    out.flush()
}

#[cfg(test)]
#[path = "pairing_layout_tests.rs"]
mod pairing_layout_tests;

#[cfg(test)]
#[path = "pairing_paint_tests.rs"]
mod pairing_paint_tests;
