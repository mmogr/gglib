//! A chat read with its branch points, and a change made to it (ADR 0017).
//!
//! Both go through the chat history service, which holds no rule of its
//! own: `gglib_core::domain::branching` decides whether a change is made in
//! place or on a new branch. A change says, in its answer, whether the chat
//! it leaves is to be answered; the page then starts an answer run
//! (`answer_saved` on `PUT /api/runs/{id}?kind=agent`).

use axum::Json;
use axum::extract::{Path, State};

use gglib_core::domain::branching::{ChatChange, ChatChanged, ChatThread};
use gglib_core::services::ChangeError;

use crate::error::HttpError;
use crate::state::AppState;

/// A chat as the page reads it: its messages, its branch points, and
/// whether it ends in a question nothing answers.
/// GET /api/conversations/:id/thread
pub(crate) async fn thread(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ChatThread>, HttpError> {
    let thread = state.core.chat_history().thread(id).await;
    Ok(Json(thread.map_err(ChangeError::from)?))
}

/// Make a change to a chat. While a reply to it is being written, an edit
/// of the question it answers is made on a new branch rather than in place:
/// a change is never refused for being busy.
/// POST /api/conversations/:id/changes
pub(crate) async fn change(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(change): Json<ChatChange>,
) -> Result<Json<ChatChanged>, HttpError> {
    let busy = state.runs.live_on(id).is_some();
    let changed = state.core.chat_history().change(id, &change, busy).await?;
    Ok(Json(changed))
}
