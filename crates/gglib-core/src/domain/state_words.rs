//! The words every surface uses for a state, written once.
//!
//! A proxy that is listening, a model that is in memory, a paired machine
//! that is reachable: each is one state with one word, and the CLI's
//! printers, the daemon's wire types and the GUI's bindings all take it from
//! here. The GUI takes its words from here as each of its screens is rebuilt
//! on the redesign, and the CLI prints no word the GUI does not know.
//!
//! On the wire each state is its `snake_case` name; [`word`](EndpointState::word)
//! is what a person reads. `gglib-cli`'s state-words tests write the whole
//! table to `contracts/vocabulary/words.json`, and the GUI's word table is
//! held to that file.

use std::fmt;

use serde::{Deserialize, Serialize};

/// What the endpoint (the proxy) is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EndpointState {
    /// Listening, and every library model answers by name.
    On,
    /// Not listening; apps cannot reach the models. Loaded models stay in
    /// memory.
    #[default]
    Off,
    /// Listening, and only the pinned model answers; other names are refused.
    Pinned,
    /// Asked to start and not yet listening. The daemon never reports it: a
    /// client shows it between sending the start and hearing the answer.
    Starting,
}

impl EndpointState {
    /// Every state, in display order.
    pub const ALL: [Self; 4] = [Self::On, Self::Off, Self::Pinned, Self::Starting];

    /// The state of a proxy that is listening, pinned or not.
    #[must_use]
    pub const fn listening(pinned: bool) -> Self {
        if pinned { Self::Pinned } else { Self::On }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Off => "Off",
            Self::Pinned => "Pinned",
            Self::Starting => "Starting",
        }
    }
}

impl fmt::Display for EndpointState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Whether a model is in memory, and what it is doing there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ResidencyState {
    /// In memory, idle.
    Loaded,
    /// Being put in memory. The resident set never reports it, since a model
    /// is not resident until the load finishes: a client shows it between
    /// asking for the load and the resident appearing.
    Loading,
    /// In memory and answering at least one request.
    Working,
    /// In memory and reading a prompt, before it answers. Known from the
    /// request's phase, not from the resident set.
    Reading,
    /// Asked for, and queued for a slot behind the models already in memory.
    Waiting,
    /// Not in memory.
    #[default]
    NotLoaded,
}

impl ResidencyState {
    /// Every state, in display order.
    pub const ALL: [Self; 6] = [
        Self::Loaded,
        Self::Loading,
        Self::Working,
        Self::Reading,
        Self::Waiting,
        Self::NotLoaded,
    ];

    /// The state of a resident with `inflight` requests holding its slot.
    #[must_use]
    pub const fn of_inflight(inflight: u32) -> Self {
        if inflight > 0 {
            Self::Working
        } else {
            Self::Loaded
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Loaded => "Loaded",
            Self::Loading => "Loading",
            Self::Working => "Working",
            Self::Reading => "Reading",
            Self::Waiting => "Waiting",
            Self::NotLoaded => "Not loaded",
        }
    }
}

impl fmt::Display for ResidencyState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Whether a paired machine can be reached from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ReachState {
    /// The tunnel is up and the far machine answers.
    Connected,
    /// The tunnel is up but the far machine has stopped answering; it
    /// reconnects on its own when the machine is back.
    Away,
    /// No tunnel to it. The pairing, if any, is remembered.
    #[default]
    Disconnected,
}

impl ReachState {
    /// Every state, in display order.
    pub const ALL: [Self; 3] = [Self::Connected, Self::Away, Self::Disconnected];

    /// The state of a connect side that is `up`, and if so whether the far
    /// machine is `away`.
    #[must_use]
    pub const fn of(up: bool, away: bool) -> Self {
        match (up, away) {
            (false, _) => Self::Disconnected,
            (true, true) => Self::Away,
            (true, false) => Self::Connected,
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Away => "Away",
            Self::Disconnected => "Disconnected",
        }
    }
}

impl fmt::Display for ReachState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listening_proxy_is_on_or_pinned_and_a_resident_works_while_it_answers() {
        assert_eq!(EndpointState::listening(false), EndpointState::On);
        assert_eq!(EndpointState::listening(true), EndpointState::Pinned);
        assert_eq!(ResidencyState::of_inflight(0), ResidencyState::Loaded);
        assert_eq!(ResidencyState::of_inflight(2), ResidencyState::Working);
        assert_eq!(ReachState::of(false, true), ReachState::Disconnected);
        assert_eq!(ReachState::of(true, true), ReachState::Away);
        assert_eq!(ReachState::of(true, false), ReachState::Connected);
    }

    #[test]
    fn every_state_is_on_the_wire_as_its_snake_case_name_and_reads_as_its_word() {
        for state in EndpointState::ALL {
            let wire = serde_json::to_string(&state).unwrap();
            assert_eq!(wire, format!("\"{}\"", state.word().to_lowercase()));
            assert_eq!(state.to_string(), state.word());
        }
        assert_eq!(
            serde_json::to_string(&ResidencyState::NotLoaded).unwrap(),
            "\"not_loaded\""
        );
        assert_eq!(ResidencyState::NotLoaded.to_string(), "Not loaded");
        assert_eq!(ReachState::default(), ReachState::Disconnected);
    }
}
