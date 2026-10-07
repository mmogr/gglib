//! Assembling the user message `gglib q` actually sends.
//!
//! The question a user types is rarely the whole prompt: `--file` or piped
//! stdin supplies the material to reason about, and a `{}` placeholder decides
//! whether that material is substituted into the question or wrapped around
//! it. That is enough branching to be worth reading on its own, away from the
//! agent-session setup that surrounds it. The continuation prompt at the end
//! of a question lives here too: it is the other place `q` reads from the
//! person rather than the agent.

use std::io::{self, BufRead, Write};

use anyhow::{Result, anyhow};

use crate::utils::input;

/// Build the user message, incorporating piped stdin or `--file` content.
///
/// `show_prompt` (`--show-prompt`) echoes the assembled message to stderr. It
/// is not a local `--verbose`: that arg id would collide with the global one
/// and leave `gglib q` with no way to turn on debug logging.
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn build_user_message(
    question: &str,
    file: Option<&str>,
    show_prompt: bool,
) -> Result<String> {
    use std::io::{self, IsTerminal, Read};

    // --file takes precedence over piped stdin.
    let context = if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read file '{path}': {e}"))?;
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    } else {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            None
        } else {
            let mut buffer = String::new();
            stdin
                .lock()
                .read_to_string(&mut buffer)
                .map_err(|e| anyhow!("failed to read from stdin: {e}"))?;
            if buffer.is_empty() {
                None
            } else {
                Some(buffer)
            }
        }
    };

    let user_message = match context {
        Some(input) => {
            if question.contains("{}") {
                question.replace("{}", &input)
            } else {
                format!("<context>\n{}\n</context>\n\n{}", input.trim(), question)
            }
        }
        None => question.to_string(),
    };

    if show_prompt {
        eprintln!("─── User Message ───");
        eprintln!("{user_message}");
        eprintln!("─── End ───\n");
    }

    Ok(user_message)
}

/// Ask whether to carry on into an interactive chat session.
///
/// The answer is read by the rule every yes/no question of the CLI is
/// ([`input::confirm_from`]), with Enter for yes; the end of input (Ctrl+D)
/// declines. Asked on stderr: stdout holds the answer `q` printed, and may be
/// a pipe.
pub(super) fn ask_continue() -> Result<bool> {
    // Flush stdout to ensure the agent's final output is fully rendered
    // before we print the prompt — prevents interleaving.
    io::stdout().flush().ok();
    eprintln!();
    continue_from(&mut io::stdin().lock(), &mut io::stderr())
}

/// [`ask_continue`]'s question, asked on `asked_on` and answered from `input`.
fn continue_from(input: &mut impl BufRead, asked_on: &mut impl Write) -> Result<bool> {
    input::confirm_from(input, asked_on, "Continue chatting?", true)
}

#[cfg(test)]
#[path = "question_input_tests.rs"]
mod tests;
