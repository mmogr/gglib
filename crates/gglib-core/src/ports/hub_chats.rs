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

use crate::domain::hub_chats::{HubChatList, HubChatOpen};

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
