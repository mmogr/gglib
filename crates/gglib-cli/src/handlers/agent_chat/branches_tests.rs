//! `/retry`, `/edit`, `/branch` and `/branches` on a session's saved chat,
//! over a database of its own: a change that would rewrite a saved reply
//! is made on a new branch, which the session goes on in, and the chat it
//! was made on is kept as it was. Whether the chat the change leaves is to
//! be answered is what the session's next turn says (`answer_saved`).

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::domain::branching::{BranchOption, BranchPoint};
use gglib_core::domain::chat::MessageRole::{Assistant, User};
use gglib_core::domain::chat::{ConversationSettings, NewMessage};

use gglib_core::domain::branching::ChatChanged;
use serde_json::{Value, json};

use super::*;
use crate::bootstrap::{CliContext, test_context};
use crate::daemon_client::STAND_IN_PORT;
use crate::handlers::agent_chat::turn::turn_stand_in::{Answer, handle, stand_in};

/// A chat holding `rows`, and the session saved to it.
async fn chat<'a>(ctx: &'a CliContext, rows: &[(MessageRole, &str)]) -> Conversation<'a> {
    let history = ctx.app.chat_history();
    let made = Conversation::create(history, None, ConversationSettings::default(), None).await;
    let conversation = made.expect("a conversation");
    for (role, content) in rows {
        let row = NewMessage {
            conversation_id: conversation.id,
            role: *role,
            content: (*content).to_owned(),
            metadata: None,
            images: Vec::new(),
        };
        history.save_message(row).await.expect("saved");
    }
    conversation
}

const ANSWERED: [(MessageRole, &str); 2] = [
    (User, "Plan a trip to Kyoto"),
    (Assistant, "Day 1: temples"),
];

/// A stand-in daemon that makes a change as `answer` says: the change it
/// was sent, as JSON, and the session's chat after `ask`.
async fn changed(
    session: &mut Conversation<'_>,
    ask: Ask,
    answer: Result<ChatChanged, (u16, Value)>,
) -> (bool, Vec<Value>) {
    let (port, taken) = stand_in(Answer::Changes(answer));
    let went = STAND_IN_PORT.scope(port, go(&handle(), session, ask)).await;
    let sent = taken
        .lock()
        .unwrap()
        .iter()
        .filter(|(line, _)| line.contains("/changes"))
        .map(|(_, body)| serde_json::from_str(body).unwrap())
        .collect();
    (went, sent)
}

const FORKED: ChatChanged = ChatChanged {
    conversation_id: 99,
    forked: true,
    answer: true,
};

/// `/retry` on a chat that ends in a reply asks the daemon to answer the
/// reply's question again, by the reply's last message, and the session
/// goes on in the chat the daemon leaves, to answer it there.
#[tokio::test]
async fn retry_asks_the_daemon_to_answer_again_and_the_session_goes_on_where_it_says() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &ANSWERED).await;
    let rows = ctx
        .app
        .chat_history()
        .get_messages(session.id)
        .await
        .unwrap();
    let reply = rows.last().unwrap().id;
    let posted = format!("POST /api/conversations/{}/changes HTTP/1.1", session.id);

    let (port, taken) = stand_in(Answer::Changes(Ok(FORKED)));
    let answer = STAND_IN_PORT
        .scope(port, go(&handle(), &mut session, Ask::Retry))
        .await;

    assert!(answer);
    assert_eq!(session.id, 99);
    let taken = taken.lock().unwrap().clone();
    assert_eq!(taken[0].0, posted);
    assert_eq!(
        serde_json::from_str::<Value>(&taken[0].1).unwrap(),
        json!({"kind": "regenerate", "message_id": reply})
    );
}

/// `/retry` on a chat that ends in a question changes nothing: the daemon
/// is asked nothing, and the question is answered where it is.
#[tokio::test]
async fn retry_answers_a_question_nothing_answers_where_it_is() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &[(User, "Plan a trip to Kyoto")]).await;
    let id = session.id;

    let (answer, sent) = changed(&mut session, Ask::Retry, Ok(FORKED)).await;

    assert!(answer);
    assert_eq!(session.id, id);
    assert!(sent.is_empty(), "{sent:?}");
}

/// `/edit` sends the last question's new text, and `/branch` the chat's
/// last message; the session follows the daemon's answer either way,
/// answering only where it says to.
#[tokio::test]
async fn edit_and_branch_send_their_change_and_follow_the_answer() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &ANSWERED).await;
    let rows = ctx
        .app
        .chat_history()
        .get_messages(session.id)
        .await
        .unwrap();
    let question = rows[0].id;

    let edit = Ask::Edit("Plan a trip to Osaka".into());
    let (answer, sent) = changed(&mut session, edit, Ok(FORKED)).await;
    assert!(answer);
    assert_eq!(session.id, 99);
    assert_eq!(
        sent,
        [json!({"kind": "edit", "message_id": question, "content": "Plan a trip to Osaka"})]
    );

    let mut session = chat(&ctx, &ANSWERED).await;
    let rows = ctx
        .app
        .chat_history()
        .get_messages(session.id)
        .await
        .unwrap();
    let last = rows[1].id;
    let copied = ChatChanged {
        conversation_id: 100,
        forked: true,
        answer: false,
    };
    let (answer, sent) = changed(&mut session, Ask::Branch, Ok(copied)).await;
    assert!(!answer);
    assert_eq!(session.id, 100);
    assert_eq!(sent, [json!({"kind": "branch", "message_id": last})]);
}

/// A change the chat cannot take asks the daemon nothing, and one the
/// daemon refuses leaves the session where it was: neither is answered.
#[tokio::test]
async fn a_change_that_cannot_be_made_leaves_the_session_where_it_was() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &[(Assistant, "Hello! Ask me anything.")]).await;
    let id = session.id;

    for ask in [Ask::Edit("Hi".into()), Ask::Edit(String::new())] {
        let (answer, sent) = changed(&mut session, ask, Ok(FORKED)).await;
        assert!(!answer);
        assert!(sent.is_empty(), "{sent:?}");
    }
    for ask in [Ask::Retry, Ask::Branch] {
        let refused = Err((
            409,
            json!({"error": "nothing to answer", "code": "conflict"}),
        ));
        let (answer, sent) = changed(&mut session, ask, refused).await;
        assert!(!answer);
        assert_eq!(sent.len(), 1);
    }
    assert_eq!(session.id, id);
}

#[test]
fn an_edit_keeps_the_images_of_the_question_it_edits() {
    let id = AttachmentId::parse(&"ab".repeat(32)).expect("an id");
    let question = Message {
        id: 7,
        conversation_id: 1,
        role: User,
        content: "What is this?".to_owned(),
        created_at: String::new(),
        metadata: None,
        origin_id: None,
        images: vec![AttachmentInfo {
            id: id.clone(),
            mime: "image/png".to_owned(),
            width: 8,
            height: 8,
        }],
    };

    let made = change_for(Ask::Edit("And this?".into()), &[question]);

    let edit = ChatChange::Edit {
        message_id: 7,
        content: "And this?".to_owned(),
        images: vec![id],
    };
    assert_eq!(made, Ok(Some(edit)));
}

fn option(conversation_id: i64, message_id: Option<i64>, preview: &str) -> BranchOption {
    BranchOption {
        conversation_id,
        message_id,
        role: message_id.map(|_| User),
        preview: preview.to_owned(),
    }
}

#[test]
fn branches_are_listed_at_each_point_with_this_chat_marked() {
    let thread = ChatThread {
        messages: Vec::new(),
        points: vec![
            BranchPoint {
                message_id: Some(3),
                index: 1,
                options: vec![
                    option(30, Some(23), "Make it cheaper"),
                    option(20, Some(3), "Make it shorter"),
                ],
            },
            BranchPoint {
                message_id: None,
                index: 1,
                options: vec![option(30, Some(24), ""), option(20, None, "")],
            },
        ],
        answerable: false,
    };

    assert_eq!(
        describe(20, &thread),
        "Branches along chat #20:\n  At the reply #3:\n    #30     Make it cheaper\n  * #20     Make it shorter\n  \
         After its last message:\n    #30     (no text)\n  * #20     (nothing here yet)\n\
         Open one with: gglib chat --continue <ID>\n"
    );
    assert_eq!(
        describe(
            5,
            &ChatThread {
                messages: Vec::new(),
                points: Vec::new(),
                answerable: false
            }
        ),
        "Chat #5 has no other branches.\n"
    );
}
