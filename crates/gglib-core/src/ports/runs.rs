//! The daemon's runs, as the surfaces that serve them see them.
//!
//! A run is one reply the daemon owns from start to end, so it survives the
//! client that asked for it leaving. `gglib-app-services` keeps them; the
//! daemon's routes and, later, the proxy's door for paired devices serve them
//! through this port, which is why the scope rule lives behind it rather than
//! in either surface.
//!
//! # Design Rules
//!
//! - A caller names its scope on every call, and the port decides what that
//!   scope may see. A device sees its own runs; this machine sees every run
//!   but may not read the events of a device's own chat run. A run on one of
//!   the hub's chats (with a `conversation_id`) belongs to the chat: this
//!   machine and every paired device may see, read and cancel it.
//! - Frames are opaque strings. Nothing behind this port puts one, or a
//!   request body, in a log line, a tracing field, an error or a file.
//! - Synchronous, except the event stream: every call is a lock and a lookup.
//!   `create` spawns the run, so it is called inside a Tokio runtime.

use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use serde_json::Value;

use crate::domain::runs::{RunInfo, RunList};

/// Who is asking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunScope {
    /// This machine: the daemon's own routes.
    Local,
    /// A paired device, by the name its key was issued under.
    Device(String),
}

impl RunScope {
    /// The device's name, or `None` for this machine.
    #[must_use]
    pub fn device(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Device(name) => Some(name),
        }
    }
}

/// One item of a run's event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    /// One logged event, numbered from 1.
    Frame {
        /// Its number in the run's log.
        seq: u32,
        /// The event, as the executor logged it.
        data: Arc<str>,
    },
    /// The run's latest preview frame, sent beside the log and never in it:
    /// it has no seq, and a reader that reconnects gets the current one once.
    Preview {
        /// The tool call it belongs to.
        tool_call_id: Arc<str>,
        /// `{"tool_call_id": .., "frame": {"mime", "step", "total", "b64"}}`.
        data: Arc<str>,
    },
    /// The run has ended; nothing follows this.
    End(RunInfo),
}

/// A run's events: the log after a cursor, live ones, then the end.
///
/// A stream that stops without a [`RunEvent::End`] means the run was dropped
/// (a forgotten device, or the daemon stopping).
pub type RunEvents = Pin<Box<dyn Stream<Item = RunEvent> + Send>>;

/// Why a runs call was refused. Every message is fixed text or ids: none
/// carries a frame or a request body.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunsError {
    /// The id is not 1 to 64 characters of `[A-Za-z0-9_-]`.
    #[error("a run id is 1 to 64 characters of letters, digits, '-' and '_'")]
    InvalidId,
    /// The request body is not a JSON object.
    #[error("a run's request body must be a JSON object")]
    InvalidBody,
    /// No run by that id is visible to the caller.
    #[error("no run has that id")]
    NotFound,
    /// The id is in use by a run the caller may not see.
    #[error("that run id is already in use")]
    IdTaken,
    /// This machine asked for the events of a paired device's own run.
    #[error("that run belongs to a paired device, so only it may read the reply")]
    NotYours,
    /// The conversation already has a run whose reply is not yet saved.
    #[error("conversation {conversation_id} already has a live reply, run {run}; stop it or wait")]
    ConversationBusy {
        /// The conversation asked for.
        conversation_id: i64,
        /// The id of the run that holds it.
        run: String,
    },
    /// Every slot holds a run that has not ended.
    #[error("32 runs are still going; cancel one or wait for one to end")]
    TooManyRuns,
    /// The daemon is stopping.
    #[error("the daemon is shutting down, so it starts no more runs")]
    ShuttingDown,
}

impl RunsError {
    /// The stable code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidId | Self::InvalidBody => "invalid_request",
            Self::NotFound => "not_found",
            Self::IdTaken | Self::ConversationBusy { .. } => "conflict",
            Self::NotYours => "not_yours",
            Self::TooManyRuns => "too_many_runs",
            Self::ShuttingDown => "shutting_down",
        }
    }

    /// The HTTP status it is answered with.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::InvalidId | Self::InvalidBody => 400,
            Self::NotYours => 403,
            Self::NotFound => 404,
            Self::IdTaken | Self::ConversationBusy { .. } => 409,
            Self::TooManyRuns => 429,
            Self::ShuttingDown => 503,
        }
    }
}

/// What `create` answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// The run.
    pub info: RunInfo,
    /// `false` when the id was already the caller's, so nothing started.
    pub created: bool,
}

/// The daemon's runs. `Debug` so it can travel in the proxy's config, as
/// the remote gateway does.
pub trait RunsPort: Send + Sync + std::fmt::Debug {
    /// Start a chat run under the client's `id`, or return the caller's run
    /// that already has it.
    ///
    /// # Errors
    ///
    /// [`RunsError::InvalidId`], [`RunsError::InvalidBody`],
    /// [`RunsError::IdTaken`] when another scope has the id,
    /// [`RunsError::TooManyRuns`], and [`RunsError::ShuttingDown`].
    fn create(&self, scope: RunScope, id: &str, body: Value) -> Result<Created, RunsError>;

    /// Every run the caller may see, newest first.
    fn list(&self, scope: &RunScope) -> RunList;

    /// One run.
    ///
    /// # Errors
    ///
    /// [`RunsError::NotFound`] when the caller may not see it.
    fn get(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError>;

    /// A run's events after `after`.
    ///
    /// # Errors
    ///
    /// [`RunsError::NotFound`], and [`RunsError::NotYours`] for this machine
    /// asking for a device's own run, one not on a hub chat.
    fn events(&self, scope: &RunScope, id: &str, after: u32) -> Result<RunEvents, RunsError>;

    /// Stop a run. Idempotent: an ended run is answered as it is.
    ///
    /// # Errors
    ///
    /// [`RunsError::NotFound`] when the caller may not see it.
    fn cancel(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError>;

    /// Cancel and drop every run of `device` at once; its open readers end.
    /// A run it started on a hub chat is the chat's and goes on, its reply
    /// saved. Returns how many runs were dropped.
    fn forget_device(&self, device: &str) -> usize;
}
