//! The run types as they cross the wire.
//!
//! An optional field is left out of the body when it has no value, and reads
//! back as empty whether it arrives absent or as `null`.

use serde::{Deserialize, Serialize};

/// What a run produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum RunKind {
    /// One chat completion.
    Chat,
    /// An agent loop, which may call tools between completions.
    Agent,
}

/// The shape of the events a run logs, which says how a reader decodes
/// them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum RunFrames {
    /// `OpenAI` chat-completion chunks, as a chat run records them. What a
    /// run says by leaving the field out, and what a reader that has never
    /// heard of the field assumes.
    #[default]
    Openai,
    /// The agent loop's events: an agent run, and a chat run started with
    /// gglib's builtins.
    Agent,
}

impl RunFrames {
    /// Whether these are `OpenAI` chunks, and so left out of the body.
    #[must_use]
    pub fn is_openai(&self) -> bool {
        *self == Self::Openai
    }
}

/// Where a run is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum RunStatus {
    /// Accepted, and waiting for a model.
    Queued,
    /// Producing events.
    InProgress,
    /// Ended with a full reply.
    Completed,
    /// Ended on an error, which the run's `error` names.
    Failed,
    /// Ended because a client asked it to stop.
    Cancelled,
}

impl RunStatus {
    /// Whether the run has ended, so no further event will be logged.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Queued | Self::InProgress => false,
            Self::Completed | Self::Failed | Self::Cancelled => true,
        }
    }
}

/// Why a run failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RunError {
    /// A stable, machine-readable code, such as `model_unavailable`.
    pub code: String,
    /// A sentence for a person to read.
    pub message: String,
}

/// One run, as the daemon reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RunInfo {
    /// The run's id, minted by the client that started it.
    pub id: String,
    /// What the run produces.
    pub kind: RunKind,
    /// Where the run is in its life.
    pub status: RunStatus,
    /// The model the run was sent to, when one was named.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The paired device that started the run; absent when it came from this
    /// machine.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// When the run was accepted, in milliseconds since the Unix epoch.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub created_at_ms: u64,
    /// When the run ended, in milliseconds since the Unix epoch; absent while
    /// it has not.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    /// The saved conversation the daemon writes the run's transcript to;
    /// absent when it writes none.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<i64>,
    /// The number of the last event logged for the run; 0 when none has been.
    pub last_seq: u32,
    /// Why the run failed; present only when `status` is `failed`.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RunError>,
    /// How to decode the run's events: `agent` for an agent run and for a
    /// chat run started with builtins; left out, and read as `openai`, for
    /// a chat run's `OpenAI` chunks. A reader resuming from a listing picks
    /// its decoder by this, not by `kind`.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<RunFrames>", optional))]
    #[serde(default, skip_serializing_if = "RunFrames::is_openai")]
    pub frames: RunFrames,
}

impl RunInfo {
    /// Whether the run holds `conversation_id`: it writes that conversation,
    /// and is not yet reported ended, which a run is only once its reply is
    /// saved. The daemon admits no second run to a held conversation, and a
    /// client that writes a conversation itself asks this of the daemon's
    /// listing once, before it starts.
    #[must_use]
    pub fn holds(&self, conversation_id: i64) -> bool {
        self.conversation_id == Some(conversation_id) && !self.status.is_terminal()
    }
}

/// A set of runs, as a listing returns them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RunList {
    /// The runs, in the order the listing chose.
    pub runs: Vec<RunInfo>,
}
