//! The hub's chats and their branches on the wire (ADR 0017): a branch
//! listed with the chat it was made from, a branch opened with its branch
//! points and its last question unanswered, a change a device makes and its
//! answer, and the turn that then answers the question. Recorded beside the
//! other bodies (`hub_chats_tests`), for both clients to replay.

use serde_json::json;

use super::hub_chats_tests::recorded;
use super::{HubChat, HubChatOpen, HubTurn};
use crate::domain::branching::{BranchOption, BranchPoint, ChatChange, ChatChanged};
use crate::domain::chat::{Conversation, Message, MessageRole};

/// A row of the branch, saved at `at`.
fn row(id: i64, role: MessageRole, content: &str, at: &str) -> Message {
    Message {
        id,
        conversation_id: 13,
        role,
        content: content.to_owned(),
        created_at: format!("2026-09-30 {at}"),
        metadata: None,
        images: Vec::new(),
        origin_id: None,
    }
}

/// The branch an edit of chat 12's second question made, as listed.
pub(super) fn listed() -> HubChat {
    HubChat {
        id: 13,
        title: "Why the build broke".to_owned(),
        model_id: Some(3),
        model: Some("qwen3-8b".to_owned()),
        updated_at: "2026-09-30 09:20:05".to_owned(),
        live_run: None,
        branch_of: Some(12),
    }
}

/// That branch opened: the first turn copied, the edited question not yet
/// answered, and the point where it and chat 12 part.
pub(super) fn opened() -> HubChatOpen {
    let conversation = Conversation {
        id: 13,
        title: "Why the build broke".to_owned(),
        branch_of: Some(12),
        lineage_id: Some(12),
        model_id: Some(3),
        system_prompt: Some("You are a helpful assistant.".to_owned()),
        settings: None,
        created_at: "2026-09-30 09:20:05".to_owned(),
        updated_at: "2026-09-30 09:20:05".to_owned(),
    };
    let option = |conversation_id, message_id, preview: &str| BranchOption {
        conversation_id,
        message_id: Some(message_id),
        role: Some(MessageRole::User),
        preview: preview.to_owned(),
    };
    HubChatOpen {
        conversation,
        messages: vec![
            row(
                50,
                MessageRole::User,
                "Why did the build break?",
                "09:12:31",
            ),
            row(
                51,
                MessageRole::Assistant,
                "A dependency moved.",
                "09:13:07",
            ),
            row(52, MessageRole::User, "And how do I pin it?", "09:20:05"),
        ],
        points: vec![BranchPoint {
            message_id: Some(52),
            index: 1,
            options: vec![
                option(12, 42, "And how do I fix it?"),
                option(13, 52, "And how do I pin it?"),
            ],
        }],
        answerable: true,
    }
}

/// The change that made it: chat 12's second question, asked again.
pub(super) fn change() -> ChatChange {
    ChatChange::Edit {
        message_id: 42,
        content: "And how do I pin it?".to_owned(),
        images: Vec::new(),
    }
}

/// Its answer: a new branch, whose last question is now to be answered.
pub(super) const fn changed() -> ChatChanged {
    ChatChanged {
        conversation_id: 13,
        forked: true,
        answer: true,
    }
}

/// The turn that answers it.
pub(super) fn answer_turn() -> HubTurn {
    HubTurn {
        conversation_id: 13,
        content: String::new(),
        images: Vec::new(),
        thinking: None,
        answer_saved: true,
    }
}

/// A branch names the chat it was made from; a chat that is none leaves the
/// key out.
#[test]
fn a_branch_names_the_chat_it_was_made_from() {
    let list = serde_json::to_value(recorded().list).unwrap();
    assert_eq!(list["chats"][2]["branch_of"], json!(12));
    assert!(list["chats"][0].get("branch_of").is_none());
}

/// A branch opened says where it and its family part, and that its last
/// question is unanswered; a chat with neither leaves both keys out.
#[test]
fn an_opened_branch_says_its_points_and_its_unanswered_question() {
    let recorded = recorded();
    let branch = serde_json::to_value(&recorded.branch_open).unwrap();
    assert_eq!(branch["answerable"], json!(true));
    assert_eq!(branch["points"][0]["index"], json!(1));
    assert_eq!(branch["conversation"]["branch_of"], json!(12));
    let plain = serde_json::to_value(&recorded.open).unwrap();
    assert!(plain.get("points").is_none() && plain.get("answerable").is_none());
}

/// A change is the body the daemon's own `/changes` takes, and is answered
/// as it answers.
#[test]
fn a_change_and_its_answer_are_the_daemons_shapes() {
    let recorded = recorded();
    assert_eq!(
        serde_json::to_value(&recorded.change).unwrap(),
        json!({ "kind": "edit", "message_id": 42, "content": "And how do I pin it?" })
    );
    assert_eq!(
        serde_json::to_value(recorded.changed).unwrap(),
        json!({ "conversation_id": 13, "forked": true, "answer": true })
    );
}

/// The turn that answers a saved question says so and carries no message;
/// a turn that adds one leaves the key out.
#[test]
fn an_answer_turn_says_so_and_a_turn_that_asks_leaves_it_out() {
    let recorded = recorded();
    assert_eq!(
        serde_json::to_value(&recorded.answer_turn).unwrap(),
        json!({ "conversation_id": 13, "content": "", "answer_saved": true })
    );
    let asks = serde_json::to_value(&recorded.turn).unwrap();
    assert!(asks.get("answer_saved").is_none());
    let read: HubTurn = serde_json::from_value(asks).unwrap();
    assert!(!read.answer_saved);
}
