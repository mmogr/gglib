//! Laying a table out for a terminal: the one owner of how wide a printed
//! table and its ruler are.
//!
//! The shape the tests are written against: every table is laid out at its
//! natural width today, whatever the terminal's, and nothing is shortened
//! or dropped.
#![allow(
    dead_code,
    reason = "the shape the tests are written against; no table is laid out by it yet"
)]

/// The columns of a terminal a piece of text takes.
pub(crate) fn display_width(text: &str) -> usize {
    console::measure_text_width(text)
}

/// A rule of `ch`, `natural` long.
pub(crate) fn rule(ch: char, natural: usize, _width: Option<usize>) -> String {
    std::iter::repeat_n(ch, natural).collect()
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
    Never,
    End { min: usize },
    Path,
    Wrap { min: usize },
}

/// One column: its header, and how it gives way when the table is too wide.
#[derive(Clone, Debug)]
pub(crate) struct Column {
    header: String,
    align: Align,
    priority: Option<u8>,
    shorten: Shorten,
    pad: usize,
}

impl Column {
    pub(crate) fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            align: Align::Left,
            priority: None,
            shorten: Shorten::Never,
            pad: 0,
        }
    }

    pub(crate) fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }

    pub(crate) fn droppable(mut self, priority: u8) -> Self {
        self.priority = Some(priority);
        self
    }

    pub(crate) fn shorten(mut self, shorten: Shorten) -> Self {
        self.shorten = shorten;
        self
    }

    pub(crate) fn pad(mut self, width: usize) -> Self {
        self.pad = width;
        self
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
    pub(crate) fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            gap: 1,
            indent: 0,
            ruler: None,
        }
    }

    pub(crate) fn gap(mut self, gap: usize) -> Self {
        self.gap = gap;
        self
    }

    pub(crate) fn indent(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }

    pub(crate) fn ruler(mut self, ch: char) -> Self {
        self.ruler = Some(ch);
        self
    }

    pub(crate) fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// The table at its natural width: `width` is not read yet.
    pub(crate) fn lay_out(&self, _width: Option<usize>) -> Laid {
        let widths: Vec<usize> = (0..self.columns.len())
            .map(|c| {
                self.rows
                    .iter()
                    .map(|row| display_width(&row[c]))
                    .chain([display_width(&self.columns[c].header), self.columns[c].pad])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let line = |cells: &[&str]| -> String {
            let mut out = " ".repeat(self.indent);
            for (c, cell) in cells.iter().enumerate() {
                if c > 0 {
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
            out.trim_end().to_owned()
        };
        let headers: Vec<&str> = self.columns.iter().map(|c| c.header.as_str()).collect();
        let header = Some(line(&headers));
        let rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| {
                let cells: Vec<&str> = row.iter().map(String::as_str).collect();
                vec![line(&cells)]
            })
            .collect();
        let ruler = self.ruler.map(|ch| {
            let widest = header
                .iter()
                .chain(rows.iter().flatten())
                .map(|l| display_width(l))
                .max()
                .unwrap_or(0);
            rule(ch, widest, None)
        });
        Laid {
            header,
            ruler,
            rows,
            blocks: false,
        }
    }
}

/// A table laid out: its header line and ruler, when it has them, and its
/// rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Laid {
    pub(crate) header: Option<String>,
    pub(crate) ruler: Option<String>,
    pub(crate) rows: Vec<Vec<String>>,
    blocks: bool,
}

impl Laid {
    pub(crate) fn is_blocks(&self) -> bool {
        self.blocks
    }

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

    pub(crate) fn text(&self) -> String {
        let mut out = String::new();
        for line in self.lines() {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// `path` as it is: nothing is shortened yet.
pub(crate) fn shorten_path(path: &str, _width: usize) -> String {
    path.to_owned()
}

#[cfg(test)]
#[path = "table_layout_tests.rs"]
mod tests;
