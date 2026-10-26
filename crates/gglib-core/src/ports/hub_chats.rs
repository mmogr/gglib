//! The hub's chats, as the proxy's door for paired devices reads them.
//!
//! Pairing is the grant: a device the tunnel edge names may list the hub's
//! chats and open one, and `gglib remote forget` takes that away with the
//! key. Nothing is copied; each call reads the hub's own rows. An image a
//! turn carries is sent once, ahead of the turn, and read back by its id.
//! A device changes a chat as the hub's own page does, by the branching
//! rules (ADR 0017): a saved reply is never rewritten. It keeps a picture it
//! had drawn again beside the one a tool drew, as the page does (ADR 0018,
//! decision 21).
//!
//! # Design Rules
//!
//! - Every error is fixed text: none carries a title, a row or a body.

use std::collections::BTreeSet;

use async_trait::async_trait;

use super::attachment_store::AttachmentError;
use super::runs::Created;
use crate::domain::attachment::{AttachmentBlob, AttachmentId, AttachmentInfo, AttachmentUpload};
use crate::domain::branching::{ChatChange, ChatChanged, Refused};
use crate::domain::hub_chats::{HubChatList, HubChatOpen, HubTurn};

/// Why a chat could not be read or changed. Fixed text only: a refusal
/// names a message by its id at most.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HubChatsError {
    /// No chat has that id.
    #[error("no chat has that id")]
    NotFound,
    /// The hub's chat history could not be read or written.
    #[error("the hub's chats could not be read")]
    Unreadable,
    /// The branching rules refuse the change; nothing was written.
    #[error(transparent)]
    Refused(#[from] Refused),
    /// A picture to keep names an image no tool row of the chat carries, or
    /// one that is not stored; nothing was written.
    #[error(
        "no tool row of the chat carries the picture it was made from, or the picture to keep is not stored"
    )]
    PictureNotFound,
}

impl HubChatsError {
    /// The stable code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Unreadable => "internal_error",
            Self::Refused(refused) => refused.code(),
            Self::PictureNotFound => "attachment_not_found",
        }
    }

    /// The HTTP status it is answered with.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Unreadable => 500,
            Self::Refused(refused) => refused.http_status(),
            // 400 and not 404, as an id a turn's body names is: a device
            // reads a 404 as a hub with no such route or chat.
            Self::PictureNotFound => 400,
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

    /// Make `change` to chat `id` as the branching rules say: in place, or
    /// on a new branch of it, which the answer names (ADR 0017). While a
    /// reply to it is being written, an edit of the question it answers is
    /// made on a branch rather than refused.
    ///
    /// # Errors
    ///
    /// [`HubChatsError::NotFound`], [`HubChatsError::Refused`] and
    /// [`HubChatsError::Unreadable`]. Nothing is written.
    async fn change(&self, id: i64, change: &ChatChange) -> Result<ChatChanged, HubChatsError>;

    /// Keep `image` as a newer picture beside `from` in chat `id`: link it
    /// to the latest tool row of the chat that carries `from`, after the
    /// images that row has, and answer the row's images in order (ADR 0018,
    /// decision 21). In place, on no branch.
    ///
    /// # Errors
    ///
    /// [`HubChatsError::NotFound`], [`HubChatsError::PictureNotFound`] and
    /// [`HubChatsError::Unreadable`]. Nothing is written.
    async fn keep_picture(
        &self,
        id: i64,
        from: &AttachmentId,
        image: &AttachmentId,
    ) -> Result<Vec<AttachmentInfo>, HubChatsError>;

    /// Store an image a device sends, for a turn to name by its id.
    ///
    /// # Errors
    ///
    /// What [`crate::services::AttachmentService::ingest`] refuses.
    async fn attach(&self, bytes: &[u8]) -> Result<AttachmentUpload, AttachmentError>;

    /// The bytes of the stored image `id`, as they were sent.
    ///
    /// # Errors
    ///
    /// [`AttachmentError::NotFound`] when no image has that id.
    async fn attachment(&self, id: &AttachmentId) -> Result<AttachmentBlob, AttachmentError>;
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

    /// Start `device`'s run `id` on a chat the device keeps itself, with
    /// gglib's builtins: `body` is its `OpenAI` chat request, whole history
    /// included, run through the agent loop on the model it names. The
    /// image tool is offered only with `draw`, and draws with
    /// `image_model` when the person chose one. The tools of the MCP
    /// servers open to paired devices are offered of those whose ids
    /// `servers` holds, which may be none, leaving the image tool to `draw`;
    /// an id it holds that is not open is dropped where the servers start,
    /// and never widens what is open. A device's chat can name only ids,
    /// never every open server. Nothing is saved to any chat here, and the run's
    /// events are the agent loop's.
    ///
    /// # Errors
    ///
    /// A [`TurnRefused`]: a body that is not a chat request, a history that
    /// cannot be read, an image that cannot be stored, a model that draws
    /// or cannot read the history's images, `draw` where nothing can draw,
    /// an `image_model` without `draw` or one that cannot draw, no free
    /// agent slot, and the runs' own.
    async fn start_chat(
        &self,
        device: &str,
        id: &str,
        body: serde_json::Value,
        draw: bool,
        image_model: Option<u32>,
        servers: BTreeSet<i64>,
    ) -> Result<Created, TurnRefused>;
}
