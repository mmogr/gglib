//! The hub's chats, as the proxy's door for paired devices reads them.
//!
//! Pairing is the grant: a device the tunnel edge names may list the hub's
//! chats and open one, and `gglib remote forget` takes that away with the
//! key. Nothing is copied; each call reads the hub's own rows.
//!
//! # Design Rules
//!
//! - Every error is fixed text: none carries a title, a row or a body.

use async_trait::async_trait;

use super::runs::Created;
use crate::domain::hub_chats::{HubChatList, HubChatOpen, HubTurn};

/// Why a chat could not be read. Fixed text only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HubChatsError {
    /// No chat has that id.
    #[error("no chat has that id")]
    NotFound,
    /// The hub's chat history could not be read.
    #[error("the hub's chats could not be read")]
    Unreadable,
}

impl HubChatsError {
    /// The stable code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unreadable => "internal_error",
        }
    }

    /// The HTTP status it is answered with.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Unreadable => 500,
        }
    }
}

/// The hub's chats. `Debug` so it can travel in the proxy's config, as the
/// runs do.
#[async_trait]
pub trait HubChatsPort: Send + Sync + std::fmt::Debug {
    /// Every chat, newest first, each with its live run if it has one.
    ///
    /// # Errors
    ///
    /// [`HubChatsError::Unreadable`].
    async fn list(&self) -> Result<HubChatList, HubChatsError>;

    /// One chat and its rows.
    ///
    /// # Errors
    ///
    /// [`HubChatsError::NotFound`] and [`HubChatsError::Unreadable`].
    async fn open(&self, id: i64) -> Result<HubChatOpen, HubChatsError>;
}

/// Why a device's turn was not started: an HTTP status, a stable code and a
/// sentence of fixed text, as the daemon's own door refuses the same run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRefused {
    /// The HTTP status it is answered with.
    pub status: u16,
    /// The stable code a client matches on.
    pub code: String,
    /// A sentence for a person; never a row, a title or a body.
    pub message: String,
}

/// Starts the agent run that answers a device's turn on a hub chat.
///
/// The agent loop is composed where the daemon's routes are, so the daemon
/// implements this and hands it to every proxy it starts: the proxy never
/// depends on the daemon's crate. The run is the device's, saved to the
/// hub's chat as the chat page's own runs are.
#[async_trait]
pub trait AgentRunStarter: Send + Sync + std::fmt::Debug {
    /// Start `device`'s run `id` adding `turn` to its chat, or answer with
    /// `device`'s run that already has the id.
    ///
    /// # Errors
    ///
    /// A [`TurnRefused`] as the daemon's door would answer: an empty
    /// message, a missing chat, a chat with a live reply (`conflict`), no
    /// free agent slot, a model that cannot be loaded, and the runs' own.
    async fn start(&self, device: &str, id: &str, turn: HubTurn) -> Result<Created, TurnRefused>;
}
