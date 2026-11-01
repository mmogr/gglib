//! The far machine's chats, for this machine's chat page: the list, one
//! chat, and a turn on one of them, which that machine runs and saves.
//!
//! Each answer is handed back whole for the daemon to pass on.

use gglib_core::domain::hub_chats::HubTurn;

use super::FarProxy;
use super::far_runs::run_path;
use crate::error::GuiError;

impl FarProxy {
    /// `GET /v1/chats`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn list_chats(&self) -> Result<reqwest::Response, GuiError> {
        self.send(self.bounded.get(self.url("/chats"))).await
    }

    /// `GET /v1/chats/{id}`.
    ///
    /// # Errors
    ///
    /// `Unavailable` when the request did not get through.
    pub async fn open_chat(&self, id: i64) -> Result<reqwest::Response, GuiError> {
        self.send(self.bounded.get(self.url(&format!("/chats/{id}"))))
            .await
    }

    /// `PUT /v1/runs/{run_id}?kind=agent` with `{conversation_id, content,
    /// images, thinking}`: a turn on one of the far machine's chats, which it
    /// runs and saves.
    ///
    /// # Errors
    ///
    /// `ValidationFailed` for a run id the hub would not accept, before
    /// anything is sent; `Unavailable` when the request did not get through.
    pub async fn add_turn(
        &self,
        run_id: &str,
        turn: &HubTurn,
    ) -> Result<reqwest::Response, GuiError> {
        let path = format!("{}?kind=agent", run_path(run_id)?);
        self.send(self.bounded.put(self.url(&path)).json(turn))
            .await
    }
}
