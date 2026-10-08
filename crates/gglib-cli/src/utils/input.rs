//! User input utilities for interactive command-line prompts.
//!
//! This module provides functions for safely collecting user input
//! including strings and confirmations.

use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};

/// Prompts the user for a string input.
///
/// Displays a prompt message and waits for the user to enter text.
/// The input is read from stdin and returned with whitespace trimmed.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<String>` - The user's input as a trimmed string
///
/// # Errors
///
/// Returns an error if reading from stdin fails.
pub(crate) fn prompt_string(prompt: &str) -> Result<String> {
    println!("{prompt}: ");

    #[cfg(test)]
    if let Ok(typed) = TYPED.try_with(|typed| *typed) {
        return Ok(typed.trim().to_string());
    }

    let mut input: String = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    Ok(input.trim().to_string())
}

#[cfg(test)]
tokio::task_local! {
    /// In a test, the line typed in answer to [`prompt_string`]. A task that
    /// sets it is answered with that each time it asks, in place of a line
    /// from stdin: the test binary's stdin is whatever the tests were run
    /// from, and one for every test in it.
    pub(crate) static TYPED: &'static str;
}

/// Prompts the user for a string input with a default value.
///
/// Displays a prompt message with a suggested default value. If the user
/// just presses Enter, the default value is returned.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
///
/// * `Result<String>` - The user's input or default value
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn prompt_string_with_default(prompt: &str, default: Option<&str>) -> Result<String> {
    if let Some(default_val) = default {
        println!("{prompt} [{default_val}]: ");
    } else {
        println!("{prompt}: ");
    }

    let mut input: String = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    let trimmed: &str = input.trim();
    if trimmed.is_empty() {
        if let Some(default_val) = default {
            Ok(default_val.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    } else {
        Ok(trimmed.to_string())
    }
}

/// Prompts the user for a yes/no confirmation.
///
/// Accepts 'y', 'yes', 'n', 'no' (case insensitive), and asks again for
/// anything else. Empty input is treated as 'no', and so is the end of input.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<bool>` - true if user confirms, false otherwise
///
/// # Errors
///
/// Returns an error if reading from stdin fails.
pub(crate) fn prompt_confirmation(prompt: &str) -> Result<bool> {
    confirm_from(&mut io::stdin().lock(), &mut io::stdout(), prompt, false)
}

/// Prompts the user for a yes/no confirmation, defaulting to yes.
///
/// Accepts 'y', 'yes', 'n', 'no' (case insensitive), and asks again for
/// anything else. Empty input is treated as 'yes'; the end of input is still
/// 'no', because a line nobody typed is not somebody pressing Enter.
///
/// The sibling of [`prompt_confirmation`], for the other kind of question.
/// That one guards an action the user has to actively want; this one offers
/// something they almost certainly do, where the cost of Enter meaning "no" is
/// that a good default goes unchosen by everybody who was not reading closely.
/// Which default applies is a property of the question, so it belongs in the
/// prompt rather than at each call site.
///
/// # Errors
///
/// Returns an error if reading from stdin fails.
pub(crate) fn prompt_confirmation_default_yes(prompt: &str) -> Result<bool> {
    confirm_from(&mut io::stdin().lock(), &mut io::stdout(), prompt, true)
}

/// Ask `prompt` on `asked_on` until a line read from `input` answers it: `y`
/// or `yes` is yes, `n` or `no` is no, in any case and with any space around
/// them, and an empty line is `default`. The end of `input` is no whatever
/// the default, since nobody is there to have agreed.
///
/// The one rule a yes/no answer is read by. The two prompts above ask on
/// stdout; a command whose stdout is its result asks on stderr through this.
///
/// # Errors
///
/// Returns an error if the question cannot be written or the answer read.
pub(crate) fn confirm_from(
    input: &mut impl BufRead,
    asked_on: &mut impl Write,
    prompt: &str,
    default: bool,
) -> Result<bool> {
    let hint = if default { "(Y/n)" } else { "(y/N)" };
    loop {
        writeln!(asked_on, "{prompt} {hint}: ").context("Failed to write the question")?;

        let mut line = String::new();
        let read = input
            .read_line(&mut line)
            .context("Failed to read user input")?;
        if read == 0 {
            return Ok(false);
        }
        match line.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "" => return Ok(default),
            _ => eprintln!("Please enter 'y' for yes or 'n' for no."),
        }
    }
}

/// Prompts the user for a positive floating-point number.
///
/// Displays a prompt message and waits for the user to enter a number.
/// Invalid or non-positive numbers will show an error and re-prompt.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
///
/// # Returns
///
/// * `Result<f64>` - The user's input as a positive float
#[allow(
    clippy::needless_continue,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn prompt_float(prompt: &str) -> Result<f64> {
    loop {
        let input: String = prompt_string(prompt)?;

        match input.parse::<f64>() {
            Ok(value) if value > 0.0 => return Ok(value),
            Ok(_) => {
                eprintln!("Please enter a positive number.");
                continue;
            }
            Err(_) => {
                eprintln!("Please enter a valid number.");
                continue;
            }
        }
    }
}

/// Prompts the user for a floating-point number with a default value.
///
/// Shows a default value and allows the user to press Enter to accept it,
/// or enter a new positive number. Invalid inputs will show an error and re-prompt.
///
/// # Arguments
///
/// * `prompt` - The message to display to the user
/// * `default` - Optional default value to suggest
///
/// # Returns
///
/// * `Result<f64>` - The user's input or default value as a positive float
#[allow(
    clippy::needless_continue,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn prompt_float_with_default(prompt: &str, default: Option<f64>) -> Result<f64> {
    loop {
        let input: String = if let Some(default_val) = default {
            prompt_string(&format!("{prompt} [{default_val:.1}]"))?
        } else {
            prompt_string(prompt)?
        };

        if input.trim().is_empty()
            && let Some(default_val) = default
        {
            return Ok(default_val);
        }

        match input.parse::<f64>() {
            Ok(value) if value > 0.0 => return Ok(value),
            Ok(_) => {
                eprintln!("Please enter a positive number.");
                continue;
            }
            Err(_) => {
                eprintln!("Please enter a valid number.");
                continue;
            }
        }
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
