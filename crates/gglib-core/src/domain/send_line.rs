//! The line above a chat's composer when a send has more to do than be
//! answered.
//!
//! Its model is out of memory, or the endpoint is off. One line, worded here
//! once; the GUI's builder is held to these words by
//! `contracts/vocabulary/send_line.json`, which this module's tests write.
//!
//! A send loads the chat's model when it is not resident, and turns the
//! endpoint on when it is off (a turn dials this machine's endpoint, which
//! loads the model it names), so nothing locks: the line says what a send
//! will do, and the composer stays live.

use serde::{Deserialize, Serialize};

use super::state_words::EndpointState;

/// Why a chat's model is not in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ModelAbsence {
    /// Not loaded: never loaded, unloaded, or swapped out.
    NotLoaded,
    /// Its server stopped on its own.
    Crashed,
}

impl ModelAbsence {
    /// Every absence, in the order the contract records them.
    pub const ALL: [Self; 2] = [Self::NotLoaded, Self::Crashed];
}

/// The line above a chat's composer for `model`, by the name the catalogue
/// shows: what is wrong, then what a send does about it.
///
/// - not loaded: `Qwen3-4B isn't loaded · sending loads it`;
/// - crashed: `Qwen3-4B stopped unexpectedly · sending loads it again`;
/// - either, with the endpoint off: `… · sending turns the endpoint on and
///   loads it` (`… loads it again`);
/// - resident, with the endpoint off: `The endpoint is Off · sending turns
///   it on`.
///
/// `None` when the model is resident and the endpoint is not off: a send is
/// answered, with nothing to say first.
#[must_use]
pub fn send_line(
    model: &str,
    absence: Option<ModelAbsence>,
    endpoint: EndpointState,
) -> Option<String> {
    let off = endpoint == EndpointState::Off;
    let Some(absence) = absence else {
        return off.then(|| {
            format!(
                "The endpoint is {} · sending turns it on",
                EndpointState::Off.word()
            )
        });
    };
    let (what, again) = match absence {
        ModelAbsence::NotLoaded => (format!("{model} isn't loaded"), ""),
        ModelAbsence::Crashed => (format!("{model} stopped unexpectedly"), " again"),
    };
    let does = if off {
        format!("sending turns the endpoint on and loads it{again}")
    } else {
        format!("sending loads it{again}")
    };
    Some(format!("{what} · {does}"))
}

#[cfg(test)]
#[path = "send_line_tests.rs"]
mod tests;
