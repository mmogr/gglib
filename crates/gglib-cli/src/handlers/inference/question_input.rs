//! Assembling the user message `gglib q` actually sends.
//!
//! The question a user types is rarely the whole prompt: `--file` or piped
//! stdin supplies the material to reason about, and a `{}` placeholder decides
//! whether that material is substituted into the question or wrapped around
//! it. That is enough branching to be worth reading on its own, away from the
//! agent-session setup that surrounds it. The continuation prompt at the end
//! of a question lives here too: it is the other place `q` reads from the
//! person rather than the agent.

use std::io::{self, Write as _};

use anyhow::{Result, anyhow};

/// Build the user message, incorporating piped stdin or `--file` content.
///
/// `show_prompt` echoes the assembled message to stderr. It used to be the
/// local `--verbose`, whose arg id collided with the global one and so left
/// `gglib q` with no way to turn on debug logging at all.
pub(crate) fn build_user_message(
    question: &str,
    file: Option<&str>,
    show_prompt: bool,
) -> Result<String> {
    use std::io::{self, IsTerminal, Read};

    // --file takes precedence over piped stdin.
    let context = if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("failed to read file '{}': {e}", path))?;
        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    } else {
        let stdin = io::stdin();
        if !stdin.is_terminal() {
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
        } else {
            None
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

/// Prompt the user to continue into an interactive chat session.
///
/// Returns `true` for 'y', 'Y', or empty input (Enter); `false` for
/// anything else.  EOF (Ctrl+D) is treated as a clean decline.
pub(super) fn ask_continue() -> Result<bool> {
    // Flush stdout to ensure the agent's final output is fully rendered
    // before we print the prompt — prevents interleaving.
    io::stdout().flush().ok();
    eprintln!();
    eprint!("[Continue chatting? (y/n)] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    let bytes = io::stdin()
        .read_line(&mut input)
        .map_err(|e| anyhow!("failed to read input: {e}"))?;

    // EOF (Ctrl+D) → treat as 'n'
    if bytes == 0 {
        eprintln!();
        return Ok(false);
    }

    let answer = input.trim();
    Ok(answer.is_empty() || answer.eq_ignore_ascii_case("y"))
}
