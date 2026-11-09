//! The words every surface uses for a state, written once.
//!
//! A proxy that is listening, a model that is in memory, a paired machine
//! that is reachable, a request that needs the key, an engine that is
//! installed, a tool server that runs, the download accelerator, the models
//! folder: each is one state with one word, and the CLI's
//! printers, the daemon's wire types and the GUI's bindings all take it from
//! here. The GUI takes its words from here as each of its screens is rebuilt
//! on the redesign, and the CLI prints no word the GUI does not know.
//!
//! On the wire each state is its `snake_case` name; [`word`](EndpointState::word)
//! is what a person reads. `gglib-cli`'s state-words tests write the whole
//! table to `contracts/vocabulary/words.json`, and the GUI's word table is
//! held to that file.

use std::fmt;

use super::mcp::McpServerStatus;

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

    /// What a pinned endpoint is said as with the model it serves: the
    /// [`Pinned`](Self::Pinned) word with the model's name.
    #[must_use]
    pub fn pinned_to(model: &str) -> String {
        format!("{} to {model}", Self::Pinned.word())
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

/// Whether a request to the endpoint must carry the API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    /// The proxy refuses a request without its key.
    Required,
    /// The proxy answers without one; a client that insists on a key may
    /// send any text.
    #[default]
    NoneNeeded,
}

impl KeyState {
    /// Every state, in display order.
    pub const ALL: [Self; 2] = [Self::Required, Self::NoneNeeded];

    /// The state of an endpoint that demands a key, or does not.
    #[must_use]
    pub const fn of(required: bool) -> Self {
        if required {
            Self::Required
        } else {
            Self::NoneNeeded
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Required => "Required",
            Self::NoneNeeded => "None needed",
        }
    }
}

impl fmt::Display for KeyState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Whether an engine (llama.cpp, stable-diffusion.cpp) is installed, and if
/// so whether its binary works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// Its server binary is there and, where gglib checks it, works.
    Installed,
    /// Its server binary is there and failed gglib's check (llama.cpp's
    /// validation; stable-diffusion.cpp's is not checked, so it is never in
    /// this state).
    NotWorking,
    /// No server binary.
    #[default]
    NotInstalled,
}

impl EngineState {
    /// Every state, in display order.
    pub const ALL: [Self; 3] = [Self::Installed, Self::NotWorking, Self::NotInstalled];

    /// The state of an engine whose binary is `installed`, and if so is
    /// `broken`.
    #[must_use]
    pub const fn of(installed: bool, broken: bool) -> Self {
        match (installed, broken) {
            (false, _) => Self::NotInstalled,
            (true, true) => Self::NotWorking,
            (true, false) => Self::Installed,
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Installed => "Installed",
            Self::NotWorking => "Installed, not working",
            Self::NotInstalled => "Not installed",
        }
    }
}

impl fmt::Display for EngineState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// What a tool server (an MCP server) is doing. A failed server's reason is
/// its status's, beside this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum McpState {
    /// Running, and its tools are offered.
    Running,
    /// Being started.
    Starting,
    /// Not running; it starts when it is next needed or asked.
    #[default]
    Stopped,
    /// It stopped on an error, or never started.
    Failed,
    /// Of a kind gglib cannot run: an SSE server.
    Unsupported,
}

impl McpState {
    /// Every state, in display order.
    pub const ALL: [Self; 5] = [
        Self::Running,
        Self::Starting,
        Self::Stopped,
        Self::Failed,
        Self::Unsupported,
    ];

    /// The state a server's `status` is in.
    #[must_use]
    pub const fn of(status: &McpServerStatus) -> Self {
        match status {
            McpServerStatus::Running => Self::Running,
            McpServerStatus::Starting => Self::Starting,
            McpServerStatus::Stopped => Self::Stopped,
            McpServerStatus::Error(_) => Self::Failed,
            McpServerStatus::Unsupported => Self::Unsupported,
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Starting => "Starting",
            Self::Stopped => "Stopped",
            Self::Failed => "Failed",
            Self::Unsupported => "Not supported yet",
        }
    }
}

impl fmt::Display for McpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Whether the optional download accelerator (`hf_xet`) is set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum FastDownloadsState {
    /// Its environment is there, so downloads take the faster path.
    On,
    /// Not set up; downloads run natively over HTTP.
    #[default]
    Off,
}

impl FastDownloadsState {
    /// Every state, in display order.
    pub const ALL: [Self; 2] = [Self::On, Self::Off];

    /// The state of an accelerator whose environment is `provisioned`, or
    /// not.
    #[must_use]
    pub const fn of(provisioned: bool) -> Self {
        if provisioned { Self::On } else { Self::Off }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

impl fmt::Display for FastDownloadsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Whether the models folder is there to write downloads into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ModelsDirState {
    /// It is there, and can be written.
    Writable,
    /// It is not there yet, and can be made: the first download makes it.
    #[default]
    Absent,
    /// It cannot be written: it is there and refuses a write, or it is not
    /// there and the folder it would be made in refuses one.
    NotWritable,
}

impl ModelsDirState {
    /// Every state, in display order.
    pub const ALL: [Self; 3] = [Self::Writable, Self::Absent, Self::NotWritable];

    /// The state of a folder that `exists` or not, and that can be written
    /// (made, for one that is not there) or not.
    #[must_use]
    pub const fn of(exists: bool, writable: bool) -> Self {
        match (exists, writable) {
            (_, false) => Self::NotWritable,
            (false, true) => Self::Absent,
            (true, true) => Self::Writable,
        }
    }

    /// The word a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Writable => "Writable",
            Self::Absent => "Will be created",
            Self::NotWritable => "Not writable",
        }
    }

    /// The word beside the folder's path where a person must know it:
    /// none for a folder that is there and writable.
    #[must_use]
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::Writable => None,
            other => Some(other.word()),
        }
    }
}

impl fmt::Display for ModelsDirState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

#[cfg(test)]
#[path = "state_words_tests.rs"]
mod tests;
