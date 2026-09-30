//! A run that replaces rows: once accepted, the rows from `replace_from` on
//! give way to the user's message in one transaction; a repeated create
//! replaces nothing; a row that is not the conversation's refuses the run.

use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::ports::RunsPort as _;

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
        };
        ids.push(state.core.chat_history().save_message(row).await.unwrap());
    }
    ids
}

fn replacing(conversation: i64, from: i64) -> Transcript {
    Transcript {
        conversation_id: Some(conversation),
        replace_from: Some(from),
    }
}

async fn launch_replacing(state: &AppState, run: &str, transcript: Transcript) -> bool {
    let (p, _) = prepared(finished_reply(), End::Finish);
    let permit = super::compose::take_permit(state).expect("a free slot");
    launch(state, run, LOCAL, transcript, p, permit)
        .await
        .unwrap()
        .created
}

#[tokio::test]
async fn an_accepted_run_replaces_the_rows_from_the_edited_message_on() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let rows = seed(
        &state,
        id,
        &[
            (MessageRole::User, "Q1"),
            (MessageRole::Assistant, "A1"),
            (MessageRole::User, "Q2"),
            (MessageRole::Assistant, "A2"),
        ],
    )
    .await;

    assert!(launch_replacing(&state, "e1", replacing(id, rows[2])).await);
    settled(&state).await;

    let contents: Vec<String> = saved(&state, id)
        .await
        .into_iter()
        .map(|r| r.content)
        .collect();
    assert_eq!(contents[..3], ["Q1", "A1", "PROMPT-SECRET"]);
    assert_eq!(
        contents
            .iter()
            .filter(|c| c.as_str() == "Q2" || c.as_str() == "A2")
            .count(),
        0
    );
}

#[tokio::test]
async fn a_repeated_create_replaces_nothing_the_second_time() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let rows = seed(
        &state,
        id,
        &[(MessageRole::User, "Q"), (MessageRole::Assistant, "A")],
    )
    .await;
    assert!(launch_replacing(&state, "e1", replacing(id, rows[0])).await);
    settled(&state).await;
    let before = saved(&state, id).await;

    let (p, _) = prepared(finished_reply(), End::Finish);
    let permit = || super::compose::take_permit(&state).expect("a free slot");
    let again = launch(
        &state,
        "e1",
        LOCAL,
        replacing(id, before[0].id),
        p,
        permit(),
    )
    .await
    .unwrap();

    assert!(!again.created);
    let after: Vec<i64> = saved(&state, id).await.iter().map(|r| r.id).collect();
    assert_eq!(after, before.iter().map(|r| r.id).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_row_the_conversation_does_not_hold_refuses_the_run_and_changes_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let other = conversation(&state).await;
    seed(&state, id, &[(MessageRole::User, "Q")]).await;
    let theirs = seed(&state, other, &[(MessageRole::User, "X")]).await;

    let (p, _) = prepared(finished_reply(), End::Finish);
    let permit = || super::compose::take_permit(&state).expect("a free slot");
    let refused = launch(&state, "e1", LOCAL, replacing(id, theirs[0]), p, permit()).await;

    let Err(crate::error::HttpError::Coded { code, .. }) = refused else {
        panic!("refused");
    };
    assert_eq!(code, "message_not_found");
    assert!(
        state.runs.list(&LOCAL).runs.is_empty(),
        "no run is left behind"
    );
    assert_eq!(saved(&state, id).await.len(), 1);
    assert_eq!(saved(&state, other).await.len(), 1);
    assert_eq!(state.agent_semaphore.available_permits(), 1);
}
