//! A device's turn that answers the question its chat already ends in, as
//! a change leaves it (ADR 0017): it runs from the saved history, adds no
//! message, and is refused on a chat that ends in no question.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::domain::hub_chats::HubTurn;

use super::hub_turn_tests::{chat, turn};
use super::plan;
use crate::error::HttpError;
use crate::handlers::agent::run_fixture::state;
use crate::state::AppState;

/// The turn that answers chat `id`'s last question.
fn answering(id: i64) -> HubTurn {
    HubTurn {
        answer_saved: true,
        ..turn(id, "")
    }
}

/// The code a refused plan is answered with.
async fn refused(state: &AppState, turn: HubTurn) -> &'static str {
    match plan(state, turn).await {
        Err(HttpError::Coded { code, .. }) => code,
        Err(other) => panic!("refused without a code: {other}"),
        Ok(_) => panic!("planned"),
    }
}

#[tokio::test]
async fn an_answer_runs_from_the_saved_question_and_adds_no_message() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let asked = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "second".to_owned(),
        metadata: None,
        images: Vec::new(),
    };
    state.core.chat_history().save_message(asked).await.unwrap();

    let plan = plan(&state, answering(id)).await.unwrap();

    let last = plan.chat.messages.last().unwrap();
    assert!(matches!(last, AgentMessage::User { content, .. } if content == "second"));
    assert_eq!(plan.chat.messages.len(), 4, "the prompt and the three rows");
    assert!(plan.transcript.answer_saved, "only the reply is saved");
}

#[tokio::test]
async fn an_answer_on_a_chat_that_ends_in_a_reply_has_nothing_to_answer() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    assert_eq!(refused(&state, answering(id)).await, "nothing_to_answer");
}

#[tokio::test]
async fn an_answer_carries_no_message_of_its_own() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let speaking = HubTurn {
        content: "second".to_owned(),
        ..answering(id)
    };
    assert_eq!(refused(&state, speaking).await, "invalid_request");
    assert_eq!(refused(&state, turn(id, "  ")).await, "invalid_request");
}
