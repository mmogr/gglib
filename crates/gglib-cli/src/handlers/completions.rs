//! Shell completion script generator.
//!
//! Generates completion scripts for the supported shells by introspecting the
//! live `Cli` command tree via [`clap::CommandFactory`]. Output is written to
//! stdout so callers can pipe it directly into their shell configuration:
//!
//! ```text
//! gglib completions fish > ~/.config/fish/completions/gglib.fish
//! gglib completions bash > ~/.bash_completion
//! gglib completions zsh  > ~/.zsh/_gglib
//! ```

use std::io::{self, Write};

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::parser::Cli;

/// Write a completion script for `shell` to stdout.
///
/// The script is buffered in memory before writing so that a broken pipe
/// (e.g. the caller piping to `head`) is handled gracefully rather than
/// causing a panic from within `clap_complete`.
pub fn execute(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    let mut buf: Vec<u8> = Vec::new();
    generate(shell, &mut cmd, bin_name, &mut buf);
    match io::stdout().write_all(&buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}
