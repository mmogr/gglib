//! The transcript writer over a database of its own: which message
//! `save_user` saves and where, what `save_reply` makes of a turn's
//! frames, and what `remember_thinking` leaves on the conversation. A whole
//! turn's rows are pinned at each surface that saves one, against
//! `transcript_turn.json`.

use gglib_core::domain::agent::{
    AssistantContent, INCOMPLETE_KEY, THINKING_DURATION_KEY, TurnUsage,
};
use gglib_core::domain::chat::{ConversationSettings, Message, MessageRole, NewConversation};
use serde_json::json;

use super::*;
use crate::test_support::test_core;

async fn conversation(history: &ChatHistoryService) -> i64 {
    let made = history.create_conversation(NewConversation {
        title: "t".to_owned(),
        ..NewConversation::default()
    });
    made.await.expect("a conversation")
}

fn user(content: &str) -> AgentMessage {
    AgentMessage::User {
        content: content.to_owned(),
        images: Vec::new(),
    }
}

/// Each row as its role, what it says and its metadata.
fn said(rows: &[Message]) -> Vec<(MessageRole, &str, Option<Value>)> {
    rows.iter()
        .map(|r| (r.role, &*r.content, r.metadata.clone()))
        .collect()
}

fn frame(event: &AgentEvent) -> String {
    serde_json::to_string(event).expect("a frame")
}

/// A turn's last message is saved when it is the user's, and only then: a
/// turn with none, or one that ends on the assistant's, saves nothing. A
/// paired device's message says which device.
#[tokio::test]
async fn only_a_users_last_message_is_saved_and_a_devices_says_which() {
    let core = test_core().await;
    let history = core.chat_history();
    let id = conversation(history).await;
    let assistant = AgentMessage::Assistant {
        content: AssistantContent {
            text: Some("hello".to_owned()),
            tool_calls: Vec::new(),
        },
    };

    for last in [None, Some(&assistant)] {
        let saved = save_user(history, id, last, Some("phone")).await;
        saved.expect("nothing to save");
    }
    assert!(history.get_messages(id).await.expect("read").is_empty());

    for device in [None, Some("phone")] {
        let saved = save_user(history, id, Some(&user("hi")), device).await;
        saved.expect("saved");
    }
    let rows = history.get_messages(id).await.expect("read");
    let phone = json!({ "device": "phone" });
    assert_eq!(
        said(&rows),
        [
            (MessageRole::User, "hi", None),
            (MessageRole::User, "hi", Some(phone))
        ]
    );
}

/// A turn that answers the question a chat ends in runs from the chat's
/// prompt and every saved message; a chat that ends in a reply, or none,
/// has nothing to answer.
#[tokio::test]
async fn an_answer_runs_from_the_saved_chat_and_only_one_that_ends_in_a_question() {
    let core = test_core().await;
    let history = core.chat_history();
    let made = history.create_conversation(NewConversation {
        title: "t".to_owned(),
        system_prompt: Some("Be brief.".to_owned()),
        ..NewConversation::default()
    });
    let id = made.await.expect("a conversation");
    let empty = answer_history(history, id).await;
    assert!(matches!(empty, Err(ChangeError::Refused(_))));
    for content in ["first", "second"] {
        let saved = save_user(history, id, Some(&user(content)), None).await;
        saved.expect("saved");
    }

    let asked = answer_history(history, id).await.expect("answerable");

    let system = AgentMessage::System {
        content: "Be brief.".to_owned(),
    };
    let as_sent = |m: &[AgentMessage]| serde_json::to_value(m).expect("a value");
    assert_eq!(
        as_sent(&asked),
        as_sent(&[system, user("first"), user("second")])
    );
    let nowhere = answer_history(history, id + 1).await;
    assert!(matches!(
        nowhere,
        Err(ChangeError::History(
            ChatHistoryError::ConversationNotFound(_)
        ))
    ));
}

/// A reply's rows are made of its frames, each with the time logged at its
/// place: a turn that reasoned says how long for only when its frames were
/// timed. Its usage, stamped before it was logged, names the model. A reply
/// that did not finish says so, and the answer is how many rows it was.
#[tokio::test]
async fn a_reply_is_its_frames_timed_by_place_stamped_and_marked_when_unfinished() {
    let core = test_core().await;
    let history = core.chat_history();
    let made_by = MadeBy {
        model: "qwen".to_owned(),
        quantization: Some("Q4_K_M".to_owned()),
        device: None,
        context_size: Some(8192),
    };
    let mut events = [
        AgentEvent::ReasoningDelta {
            content: "Hm.".to_owned(),
        },
        AgentEvent::TurnUsage(TurnUsage::default()),
        AgentEvent::TextDelta {
            content: "Done.".to_owned(),
        },
    ];
    let timed = FrameTimes::new();
    let frames: Vec<String> = events
        .iter_mut()
        .map(|event| {
            made_by.stamp(event);
            timed.logged();
            frame(event)
        })
        .collect();
    assert_eq!(frames[0], r#"{"type":"reasoning_delta","content":"Hm."}"#);

    for (times, finished) in [(timed, true), (FrameTimes::new(), false)] {
        let id = conversation(history).await;
        let logged = frames.iter().map(String::as_str);

        let total = save_reply(history, id, logged, &times, finished).await;

        assert_eq!(total.expect("saved"), 1);
        let rows = history.get_messages(id).await.expect("read");
        let made = rows[0].metadata.as_ref().expect("how it was made");
        assert_eq!(
            (rows[0].role, &*rows[0].content),
            (MessageRole::Assistant, "Done.")
        );
        assert_eq!(made[MADE_KEYS.model], "qwen");
        assert_eq!(made[MADE_KEYS.quantization], "Q4_K_M");
        assert_eq!(made[MADE_KEYS.context_size], 8192);
        assert_eq!(made[THINKING_DURATION_KEY].is_number(), finished, "{made}");
        assert_eq!(
            made.get(INCOMPLETE_KEY),
            (!finished).then_some(&json!(true))
        );
    }
}

/// A Thinking choice is one field of the conversation's settings: `off` is
/// remembered, and nothing forgets it, beside what the settings already
/// held. A conversation that is not there is not a failure.
#[tokio::test]
async fn a_thinking_choice_is_remembered_and_forgotten_beside_the_other_settings() {
    let core = test_core().await;
    let history = core.chat_history();
    let made = history.create_conversation(NewConversation {
        title: "t".to_owned(),
        settings: Some(ConversationSettings {
            model_name: Some("qwen".to_owned()),
            ..ConversationSettings::default()
        }),
        ..NewConversation::default()
    });
    let id = made.await.expect("a conversation");

    for choice in [Some(Thinking::Off), None] {
        remember_thinking(history, id, choice).await;

        let read = history.get_conversation(id).await.expect("read");
        let settings = read.expect("there").settings.expect("settings");
        assert_eq!(
            (settings.thinking, settings.model_name.as_deref()),
            (choice, Some("qwen"))
        );
    }
    remember_thinking(history, id + 1, Some(Thinking::Off)).await;
}
