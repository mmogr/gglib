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

use std::io;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::parser::Cli;

/// Write a completion script for `shell` to stdout.
pub fn execute(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}
