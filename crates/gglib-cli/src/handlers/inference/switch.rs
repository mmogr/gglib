//! Starting the endpoint when it may be on already, in another shape.
//!
//! `gglib proxy`, `gglib serve` and `gglib up` each ask the daemon for the
//! endpoint they want. The daemon answers with the endpoint as asked, or
//! says it is on otherwise and how (`endpoint_differs`): another model, every
//! model, another port or sampling. Switching it cuts off the clients
//! connected now, so this asks first, in a terminal; `-y` is a yes, and a
//! run with nobody to ask changes nothing and says how to switch. The daemon
//! does the switch, stop and start, as one operation (`replace`).
//!
//! The words for what runs and what switching does are the daemon's
//! ([`gglib_app_services::EndpointChange`]); the question and the refusal
//! around them are this command's, because they name its flag.

use std::io::IsTerminal;

use anyhow::Result;
use gglib_app_services::{EndpointChange, shape_words};

/// A yes/no question asked of a person, Enter being yes.
pub(crate) type Ask = Box<dyn FnMut(&str) -> Result<bool> + Send>;

/// Whether a switch may go ahead: `-y`, or a person who answers.
pub(crate) struct Consent {
    /// `-y`: switch without asking.
    pub yes: bool,
    /// Whether there is somebody to ask: one who reads the question and
    /// types the answer.
    pub terminal: bool,
    /// Ask a yes/no question, Enter being yes.
    pub ask: Ask,
    /// This command as it was typed, which the refusal names with `-y`.
    pub command: String,
}

impl Consent {
    /// The consent of whoever runs this command: `yes` as typed, asked on
    /// stderr, since stdout may be a command's result. There is somebody to
    /// ask only when both the answer's stream and the question's are a
    /// terminal: a question written to a log file would wait on a keyboard
    /// with nothing on the screen.
    pub(crate) fn here(yes: bool) -> Self {
        Self {
            yes,
            terminal: somebody_to_ask(
                std::io::stdin().is_terminal(),
                std::io::stderr().is_terminal(),
            ),
            command: command_line(std::env::args().skip(1)),
            ask: Box::new(|question| {
                crate::utils::input::confirm_from(
                    &mut std::io::stdin().lock(),
                    &mut std::io::stderr(),
                    question,
                    true,
                )
            }),
        }
    }

    /// Whether switching the endpoint as `change` says may go ahead.
    ///
    /// # Errors
    ///
    /// When nobody is there to ask and `-y` was not given, with
    /// [`refusal`]'s sentence; and when the question cannot be asked.
    pub(crate) fn allows(&mut self, change: &EndpointChange) -> Result<bool> {
        if self.yes {
            return Ok(true);
        }
        anyhow::ensure!(self.terminal, "{}", refusal(change, &self.command));
        (self.ask)(&question(change))
    }
}

/// Whether a person can be asked: the answer is read from stdin and the
/// question written to stderr, so both must be a terminal.
pub(crate) const fn somebody_to_ask(stdin_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdin_is_terminal && stderr_is_terminal
}

/// `gglib` and `args`, as a shell takes them back: an argument with a space
/// or a character a shell reads is single-quoted.
pub(crate) fn command_line(args: impl IntoIterator<Item = String>) -> String {
    let mut line = String::from("gglib");
    for arg in args {
        line.push(' ');
        let plain = !arg.is_empty()
            && arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@+%".contains(c));
        if plain {
            line.push_str(&arg);
        } else {
            line.push('\'');
            line.push_str(&arg.replace('\'', "'\\''"));
            line.push('\'');
        }
    }
    line
}

/// What is asked: what runs, what switching does, and what it costs.
pub(crate) fn question(change: &EndpointChange) -> String {
    let cost = if change.moves {
        "Clients connected now are cut off until they are pointed at the new address."
    } else {
        "Clients connected now are cut off for a moment."
    };
    format!(
        "  The endpoint is {}. {}? {cost}",
        change.now,
        capitalized(&change.action)
    )
}

/// What is said for a no.
pub(crate) fn kept(change: &EndpointChange) -> String {
    format!("  Nothing changed: the endpoint is still {}.", change.now)
}

/// What a run with nobody to ask is refused with: one sentence naming the
/// command, as typed, with `-y`.
pub(crate) fn refusal(change: &EndpointChange, command: &str) -> String {
    format!(
        "The endpoint is {}, so nothing was changed: to {}, run `{command} -y`.",
        change.now, change.action
    )
}

/// What `gglib up` says of an endpoint on otherwise, which it leaves as it
/// is, and the command that would `change` it as `up` asked (`apply`).
pub(crate) fn left(change: &EndpointChange, apply: &str) -> [String; 2] {
    [
        format!(
            "  The endpoint is {}, so gglib up left it as it is and did not {}.",
            change.now, change.action
        ),
        format!("  To do that, run: {apply}"),
    ]
}

/// What is said when the endpoint was already as asked.
pub(crate) fn unchanged(port: u16, pinned: Option<&str>) -> String {
    format!(
        "  The endpoint is already {}, as asked.",
        shape_words(port, pinned)
    )
}

/// The closing line of a command that left the endpoint serving one model:
/// how to serve every model again, as `gglib up`'s list names a command.
pub(crate) fn way_back() -> String {
    format!("    {:<36}serve every model again", "gglib proxy")
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
#[path = "switch_tests.rs"]
mod tests;
