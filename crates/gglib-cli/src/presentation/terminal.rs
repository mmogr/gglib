//! The width of the terminal the CLI prints to: the one place it is read.
//!
//! Everything that lays text out for a terminal takes the width as a value
//! ([`super::table_layout`]), so it is tested without a terminal on every OS;
//! only the handlers call these. `None` means the stream is not a terminal
//! (piped or redirected) or its size could not be read, and a table is then
//! printed whole. `COLUMNS` is not read: neither gglib nor clap (built
//! without `wrap_help`) has ever honoured it, and a shell or CI that exports
//! it would otherwise cut a table a script reads from a pipe.

/// The width of standard output, in columns, while it is a terminal.
pub(crate) fn stdout_width() -> Option<usize> {
    width_of(&console::Term::stdout())
}

/// The width of standard error, in columns, while it is a terminal: for a
/// table printed there.
pub(crate) fn stderr_width() -> Option<usize> {
    width_of(&console::Term::stderr())
}

/// `term`'s width; `console` answers `None` for a stream that is not a
/// terminal, on every OS.
fn width_of(term: &console::Term) -> Option<usize> {
    term.size_checked()
        .map(|(_rows, columns)| usize::from(columns))
}
