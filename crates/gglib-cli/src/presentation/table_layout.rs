//! Laying a table out for a terminal: the one owner of how wide a printed
//! table and its ruler are.
//!
//! A [`Table`] is its [`Column`]s and its rows of cells. [`Table::lay_out`]
//! takes the width the table must fit as a value, from
//! [`super::terminal`] in a handler and from the test in a test, and returns
//! lines no wider than it:
//!
//! 1. Every column as wide as its padding or its widest cell, when that fits.
//! 2. Otherwise the padding goes, and what may be shortened is shortened: a
//!    name at its end, a path in its middle (keeping the file name whole),
//!    prose wrapped onto more lines; never below each column's floor.
//! 3. When the floors do not fit either, the least important column is
//!    dropped and step 2 is tried again, until only the columns that are
//!    never dropped are left.
//! 4. When even those do not fit, each row is printed as a block of
//!    `label: value` lines.
//!
//! With no width (the output is not a terminal) nothing is shortened or
//! dropped: a script reading a pipe gets every column and every cell whole.
//! The ruler under the header is as wide as the table, and never wider than
//! the width. Widths are display widths: a CJK character or an emoji takes
//! two columns, and colour codes take none.

use std::borrow::Cow;

/// The mark left where text was taken out of a cell.
const MARK: &str = "\u{2026}";

/// The columns of a terminal a piece of text takes.
pub(crate) fn display_width(text: &str) -> usize {
    console::measure_text_width(text)
}

/// A rule of `ch`, `natural` long, or as wide as the terminal when that is
/// narrower. For the rules that stand between sections rather than under a
/// table's header.
pub(crate) fn rule(ch: char, natural: usize, width: Option<usize>) -> String {
    let long = width.map_or(natural, |width| natural.min(width));
    std::iter::repeat_n(ch, long).collect()
}

/// Which side of its column a cell sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Right,
}

/// How a column's cells may be made narrower when the table does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shorten {
    /// Never: the column is dropped instead.
    Never,
    /// Cut at the end, marked with `…`, to no fewer than `min` columns: a
    /// name.
    End { min: usize },
    /// Cut in the middle, marked with `…`, keeping the file name whole: a
    /// path, of any OS's spelling, since one machine may show another's.
    Path,
    /// Wrapped onto more lines at spaces, no narrower than `min` columns:
    /// prose, whose every word matters. A colour code stays with the word
    /// it touches; only a word wider than the column is split, so no
    /// coloured word may be.
    Wrap { min: usize },
}

/// One column: its header, and how it gives way when the table is too wide.
#[derive(Clone, Debug)]
pub(crate) struct Column {
    header: String,
    align: Align,
    /// `None` for a column that is never dropped; otherwise the lower, the
    /// sooner it is dropped.
    priority: Option<u8>,
    shorten: Shorten,
    /// The width the column is padded to when the table fits.
    pad: usize,
}

impl Column {
    /// A left-aligned column under `header` that is never shortened or
    /// dropped.
    pub(crate) fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            align: Align::Left,
            priority: None,
            shorten: Shorten::Never,
            pad: 0,
        }
    }

    /// The cells sit on the right of the column.
    pub(crate) fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }

    /// The column may be dropped; the lowest `priority` goes first.
    pub(crate) fn droppable(mut self, priority: u8) -> Self {
        self.priority = Some(priority);
        self
    }

    /// How the column's cells may be shortened.
    pub(crate) fn shorten(mut self, shorten: Shorten) -> Self {
        self.shorten = shorten;
        self
    }

    /// The column is padded to `width` when the table fits.
    pub(crate) fn pad(mut self, width: usize) -> Self {
        self.pad = width;
        self
    }

    /// How much this column matters when room is handed out: a column never
    /// dropped before any that may be.
    fn importance(&self) -> u16 {
        self.priority.map_or(u16::MAX, u16::from)
    }
}

/// A table: its columns, its rows, and how it is spaced.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    gap: usize,
    indent: usize,
    ruler: Option<char>,
}

impl Table {
    /// A table of `columns`, one space between them, no indent and no ruler.
    pub(crate) fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            gap: 1,
            indent: 0,
            ruler: None,
        }
    }

    /// `gap` spaces between columns.
    pub(crate) fn gap(mut self, gap: usize) -> Self {
        self.gap = gap;
        self
    }

    /// Every line starts with `indent` spaces.
    pub(crate) fn indent(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }

    /// A rule of `ch` under the header.
    pub(crate) fn ruler(mut self, ch: char) -> Self {
        self.ruler = Some(ch);
        self
    }

    /// Add a row: one cell per column, in the columns' order. A cell may
    /// carry colour codes, which take no columns; a path's may not.
    pub(crate) fn row(&mut self, cells: Vec<String>) {
        debug_assert_eq!(cells.len(), self.columns.len(), "one cell per column");
        self.rows.push(cells);
    }

    /// The table as lines no wider than `width`, or whole when `width` is
    /// `None` (the output is not a terminal). See the module's docs for the
    /// order in which it gives way.
    pub(crate) fn lay_out(&self, width: Option<usize>) -> Laid {
        let natural: Vec<usize> = (0..self.columns.len()).map(|c| self.natural(c)).collect();
        let Some(limit) = width else {
            let all: Vec<usize> = (0..self.columns.len()).collect();
            return self.in_columns(&all, &natural, None);
        };
        match self.fit(limit, &natural) {
            Some((visible, widths)) => self.in_columns(&visible, &widths, Some(limit)),
            None => self.in_blocks(limit),
        }
    }

    /// The widest of column `c`'s header and cells.
    fn tight(&self, c: usize) -> usize {
        self.rows
            .iter()
            .map(|row| display_width(&row[c]))
            .chain(std::iter::once(display_width(&self.columns[c].header)))
            .max()
            .unwrap_or(0)
    }

    /// Column `c`'s width when there is room: its padding or its widest.
    fn natural(&self, c: usize) -> usize {
        self.tight(c).max(self.columns[c].pad)
    }

    /// The narrowest column `c` may be made.
    fn floor(&self, c: usize) -> usize {
        let column = &self.columns[c];
        let header = display_width(&column.header);
        let floor = match column.shorten {
            Shorten::Never => return self.tight(c),
            Shorten::End { min } | Shorten::Wrap { min } => min.max(header),
            Shorten::Path => self
                .rows
                .iter()
                .map(|row| path_floor(&row[c]))
                .max()
                .unwrap_or(0)
                .max(header),
        };
        floor.min(self.tight(c))
    }

    /// The columns kept and their widths at `limit`, or `None` when even the
    /// columns never dropped do not fit at their floors.
    fn fit(&self, limit: usize, natural: &[usize]) -> Option<(Vec<usize>, Vec<usize>)> {
        let mut visible: Vec<usize> = (0..self.columns.len()).collect();
        loop {
            let spacing = self.indent + self.gap * visible.len().saturating_sub(1);
            let room = limit.saturating_sub(spacing);
            let wanted: usize = visible.iter().map(|&c| natural[c]).sum();
            if spacing + wanted <= limit {
                return Some((visible, natural.to_vec()));
            }
            let mut widths: Vec<usize> = (0..self.columns.len()).map(|c| self.floor(c)).collect();
            let least: usize = visible.iter().map(|&c| widths[c]).sum();
            if spacing + least <= limit {
                let mut spare = room - least;
                // Room goes back to the columns that matter most first, up
                // to their widest cell, and then to the padding.
                let mut by_importance = visible.clone();
                by_importance.sort_by_key(|&c| std::cmp::Reverse(self.columns[c].importance()));
                for c in by_importance {
                    let grow = (self.tight(c) - widths[c]).min(spare);
                    widths[c] += grow;
                    spare -= grow;
                }
                for &c in &visible {
                    let grow = (natural[c] - widths[c]).min(spare);
                    widths[c] += grow;
                    spare -= grow;
                }
                return Some((visible, widths));
            }
            // The least important droppable column goes; of equals, the
            // rightmost.
            let drop = visible
                .iter()
                .copied()
                .filter(|&c| self.columns[c].priority.is_some())
                .min_by_key(|&c| (self.columns[c].importance(), std::cmp::Reverse(c)))?;
            visible.retain(|&c| c != drop);
        }
    }

    /// The header, its ruler and the rows, with `visible` columns at
    /// `widths`, each line cut to `limit` as a last guard.
    fn in_columns(&self, visible: &[usize], widths: &[usize], limit: Option<usize>) -> Laid {
        let line = |cells: &[&str]| -> String {
            let mut out = " ".repeat(self.indent);
            for (i, (&c, cell)) in visible.iter().zip(cells).enumerate() {
                if i > 0 {
                    out.push_str(&" ".repeat(self.gap));
                }
                let fill = " ".repeat(widths[c].saturating_sub(display_width(cell)));
                match self.columns[c].align {
                    Align::Left => {
                        out.push_str(cell);
                        out.push_str(&fill);
                    }
                    Align::Right => {
                        out.push_str(&fill);
                        out.push_str(cell);
                    }
                }
            }
            guard(out.trim_end(), limit)
        };

        let headers: Vec<&str> = visible
            .iter()
            .map(|&c| self.columns[c].header.as_str())
            .collect();
        let header = headers
            .iter()
            .any(|h| !h.is_empty())
            .then(|| line(&headers));

        let rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<Vec<String>> = visible
                    .iter()
                    .map(|&c| fit_cell(&row[c], self.columns[c].shorten, widths[c]))
                    .collect();
                let height = cells.iter().map(Vec::len).max().unwrap_or(1);
                (0..height)
                    .map(|k| {
                        let parts: Vec<&str> = cells
                            .iter()
                            .map(|lines| lines.get(k).map_or("", String::as_str))
                            .collect();
                        line(&parts)
                    })
                    .collect()
            })
            .collect();

        let ruler = self.ruler.map(|ch| {
            let widest = header
                .iter()
                .chain(rows.iter().flatten())
                .map(|l| display_width(l))
                .max()
                .unwrap_or(0);
            rule(ch, widest, limit)
        });
        Laid {
            header,
            ruler,
            rows,
            blocks: false,
        }
    }

    /// Each row as `label: value` lines, every column kept, for a width the
    /// table cannot fit.
    fn in_blocks(&self, limit: usize) -> Laid {
        let label_width = self
            .columns
            .iter()
            .map(|c| display_width(&c.header))
            .max()
            .unwrap_or(0)
            + 1;
        let indent = " ".repeat(self.indent);
        let beside = limit.saturating_sub(self.indent + label_width + 1);
        // Too little room beside the labels: each value goes under its label.
        let (budget, under) = if beside < 8 {
            (limit.saturating_sub(self.indent + 2), true)
        } else {
            (beside, false)
        };
        let rows = self
            .rows
            .iter()
            .map(|row| {
                let mut lines = Vec::new();
                for (column, cell) in self.columns.iter().zip(row) {
                    let label = format!("{}:", column.header);
                    let values = fit_cell(cell, column.shorten, budget);
                    if under {
                        lines.push(guard(&format!("{indent}{label}"), Some(limit)));
                        for value in values {
                            lines.push(guard(&format!("{indent}  {value}"), Some(limit)));
                        }
                        continue;
                    }
                    for (k, value) in values.iter().enumerate() {
                        let head = if k == 0 { label.as_str() } else { "" };
                        let fill = " ".repeat(label_width - display_width(head) + 1);
                        let line = format!("{indent}{head}{fill}{value}");
                        lines.push(guard(line.trim_end(), Some(limit)));
                    }
                }
                lines
            })
            .collect();
        Laid {
            header: None,
            ruler: None,
            rows,
            blocks: true,
        }
    }
}

/// A table laid out: its header line and ruler, when it has them, and its
/// rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Laid {
    /// The header line; `None` when the rows are blocks or no column has a
    /// header.
    pub(crate) header: Option<String>,
    /// The rule under the header.
    pub(crate) ruler: Option<String>,
    /// One entry per row, in order: its line, or lines when a cell wraps, or
    /// its block.
    pub(crate) rows: Vec<Vec<String>>,
    blocks: bool,
}

impl Laid {
    /// Whether each row is a block of `label: value` lines.
    pub(crate) fn is_blocks(&self) -> bool {
        self.blocks
    }

    /// Every line, in order, with a blank line between blocks.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.header.iter().chain(&self.ruler).cloned().collect();
        for (i, row) in self.rows.iter().enumerate() {
            if self.blocks && i > 0 {
                out.push(String::new());
            }
            out.extend(row.iter().cloned());
        }
        out
    }

    /// [`Self::lines`], each ended by a newline.
    pub(crate) fn text(&self) -> String {
        let mut out = String::new();
        for line in self.lines() {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// `line` cut to `limit`, which a laid-out line reaches only when the width
/// is narrower than the indent and a label.
fn guard(line: &str, limit: Option<usize>) -> String {
    limit.map_or_else(
        || line.to_owned(),
        |limit| cut_end(line, limit).into_owned(),
    )
}

/// `cell` made to fit `width`, as one line or, for prose, several.
fn fit_cell(cell: &str, shorten: Shorten, width: usize) -> Vec<String> {
    if display_width(cell) <= width {
        return vec![cell.to_owned()];
    }
    match shorten {
        Shorten::Path => vec![shorten_path(cell, width)],
        Shorten::Wrap { .. } => wrap(cell, width),
        Shorten::Never | Shorten::End { .. } => vec![cut_end(cell, width).into_owned()],
    }
}

/// `text` cut at its end to `width`, marked with `…`.
fn cut_end(text: &str, width: usize) -> Cow<'_, str> {
    console::truncate_str(text, width, MARK)
}

/// Where a path's file name starts: after its last `/` or `\`, whichever
/// OS spelled it.
fn file_name_at(path: &str) -> usize {
    path.rfind(['/', '\\']).map_or(0, |at| at + 1)
}

/// The widest a path column's floor is, however long a file name: one
/// model with a very long file name must not cost every row its path.
const PATH_FLOOR: usize = 32;

/// The narrowest `path` is shortened to: `…` and its file name, with the
/// separator before it, or [`PATH_FLOOR`] when that is wider.
fn path_floor(path: &str) -> usize {
    let at = file_name_at(path);
    let whole = display_width(path);
    if at == 0 {
        return whole.min(PATH_FLOOR);
    }
    whole
        .min(display_width(MARK) + display_width(&path[at - 1..]))
        .min(PATH_FLOOR)
}

/// `path` cut in the middle to `width`: its start, `…`, and its end, the
/// file name kept whole. A file name wider than `width` is itself cut in
/// its middle, so its start and its ending (the quantization and the
/// extension, in a model's) are both kept.
pub(crate) fn shorten_path(path: &str, width: usize) -> String {
    if display_width(path) <= width {
        return path.to_owned();
    }
    let at = file_name_at(path);
    let tail = &path[at.saturating_sub(1)..];
    let tail_width = display_width(tail);
    if at == 0 || display_width(MARK) + tail_width > width {
        return cut_middle(&path[at..], width);
    }
    let room = width - display_width(MARK) - tail_width;
    format!("{}{MARK}{tail}", first_columns(&path[..at - 1], room))
}

/// `text` cut to `width` with `…` in its middle: its start and its end,
/// the end given the odd column.
fn cut_middle(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_owned();
    }
    let keep = width.saturating_sub(display_width(MARK));
    let end = last_columns(text, keep.div_ceil(2));
    let start = first_columns(text, keep - display_width(end));
    format!("{start}{MARK}{end}")
}

/// The longest start of `text` no wider than `columns`.
fn first_columns(text: &str, columns: usize) -> &str {
    let mut used = 0;
    let mut buf = [0u8; 4];
    for (at, ch) in text.char_indices() {
        used += display_width(ch.encode_utf8(&mut buf));
        if used > columns {
            return &text[..at];
        }
    }
    text
}

/// The longest end of `text` no wider than `columns`.
fn last_columns(text: &str, columns: usize) -> &str {
    let mut used = 0;
    let mut buf = [0u8; 4];
    for (at, ch) in text.char_indices().rev() {
        used += display_width(ch.encode_utf8(&mut buf));
        if used > columns {
            return &text[at + ch.len_utf8()..];
        }
    }
    text
}

/// `text` wrapped at spaces into lines no wider than `width`; a word wider
/// than the line is split. For a cell that wraps, and for prose printed
/// between a table's rows.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            let sep = usize::from(!current.is_empty());
            if display_width(&current) + sep + display_width(word) <= width {
                if sep == 1 {
                    current.push(' ');
                }
                current.push_str(word);
                break;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                continue;
            }
            // The word alone is wider than a line: split it.
            let head = first_columns(word, width);
            let head = if head.is_empty() {
                &word[..word.chars().next().map_or(0, char::len_utf8)]
            } else {
                head
            };
            lines.push(head.to_owned());
            word = &word[head.len()..];
            if word.is_empty() {
                break;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
#[path = "table_layout_tests.rs"]
mod tests;
