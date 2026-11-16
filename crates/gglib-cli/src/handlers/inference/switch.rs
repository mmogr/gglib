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
    /// Whether there is somebody to ask.
    pub terminal: bool,
    /// Ask a yes/no question, Enter being yes.
    pub ask: Ask,
}

impl Consent {
    /// The consent of whoever runs this command: `yes` as typed, asked on
    /// stderr, since stdout may be a command's result.
    pub(crate) fn here(yes: bool) -> Self {
        Self {
            yes,
            terminal: std::io::stdin().is_terminal(),
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
        let _ = (self.yes, self.terminal, refusal(change));
        (self.ask)(&question(change)).map(|_| false)
    }
}

/// What is asked: what runs, what switching does, and what it costs.
pub(crate) fn question(change: &EndpointChange) -> String {
    let _ = change;
    String::new()
}

/// What is said for a no.
pub(crate) fn kept(change: &EndpointChange) -> String {
    let _ = change;
    String::new()
}

/// What a run with nobody to ask is refused with: one sentence naming `-y`.
pub(crate) fn refusal(change: &EndpointChange) -> String {
    let _ = change;
    String::new()
}

/// What `gglib up` says of an endpoint on otherwise, which it leaves as it is.
pub(crate) fn left(change: &EndpointChange) -> String {
    let _ = change;
    String::new()
}

/// What is said when the endpoint was already as asked.
pub(crate) fn unchanged(port: u16, pinned: Option<&str>) -> String {
    let _ = shape_words(port, pinned);
    String::new()
}

/// The closing line of a command that left the endpoint serving one model:
/// how to serve every model again, as `gglib up`'s list names a command.
pub(crate) fn way_back() -> String {
    String::new()
}

#[cfg(test)]
#[path = "switch_tests.rs"]
mod tests;
