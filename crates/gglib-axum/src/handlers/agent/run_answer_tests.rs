//! A run that answers the question its conversation ends in: it saves no
//! message of its own, its reply follows the question, and a repeated
//! create answers once.

use gglib_core::domain::chat::{MessageRole, NewMessage};

use super::launch::{Transcript, launch};
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, prepared, saved, settled, state,
};
use crate::state::AppState;

async fn seed(state: &AppState, id: i64, rows: &[(MessageRole, &str)]) -> Vec<i64> {
    let mut ids = Vec::new();
    for (role, content) in rows {
        let row = NewMessage {
            conversation_id: id,
            role: *role,
            content: (*content).to_owned(),
            metadata: None,
            images: Vec::new(),
        };
        ids.push(state.core.chat_history().save_message(row).await.unwrap());
    }
    ids
}

fn answering(conversation: i64) -> Transcript {
    Transcript {
        conversation_id: Some(conversation),
        answer_saved: true,
        remember: None,
    }
}

async fn launch_answering(state: &AppState, run: &str, conversation: i64) -> bool {
    let (p, _) = prepared(finished_reply(), End::Finish);
    let permit = super::compose::take_permit(state).expect("a free slot");
    launch(state, run, LOCAL, answering(conversation), p, permit)
        .await
        .unwrap()
        .created
}

#[tokio::test]
async fn an_answer_saves_no_message_of_its_own_and_its_reply_follows_the_question() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    seed(
        &state,
        id,
        &[
            (MessageRole::User, "Q1"),
            (MessageRole::Assistant, "A1"),
            (MessageRole::User, "Q2"),
        ],
    )
    .await;

    assert!(launch_answering(&state, "e1", id).await);
    settled(&state).await;

    let rows = saved(&state, id).await;
    let contents: Vec<&str> = rows.iter().map(|r| r.content.as_str()).collect();
    assert_eq!(contents[..3], ["Q1", "A1", "Q2"]);
    assert!(
        !contents.contains(&"PROMPT-SECRET"),
        "the run's own message is not saved: {contents:?}"
    );
    assert!(rows[3..].iter().all(|r| r.role != MessageRole::User));
}

#[tokio::test]
async fn a_repeated_create_answers_once() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    seed(&state, id, &[(MessageRole::User, "Q")]).await;
    assert!(launch_answering(&state, "e1", id).await);
    settled(&state).await;
    let before: Vec<i64> = saved(&state, id).await.iter().map(|r| r.id).collect();

    assert!(!launch_answering(&state, "e1", id).await);

    let after: Vec<i64> = saved(&state, id).await.iter().map(|r| r.id).collect();
    assert_eq!(after, before);
}
