//! `/retry`, `/edit`, `/branch` and `/branches` on a session's saved chat,
//! over a database of its own: a change that would rewrite a saved reply
//! is made on a new branch, which the session goes on in, and the chat it
//! was made on is kept as it was. Whether the chat the change leaves is to
//! be answered is what the session's next turn says (`answer_saved`).

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::domain::branching::{BranchOption, BranchPoint};
use gglib_core::domain::chat::MessageRole::{Assistant, User};
use gglib_core::domain::chat::{ConversationSettings, NewMessage};

use super::*;
use crate::bootstrap::{CliContext, test_context};

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

async fn contents(ctx: &CliContext, id: i64) -> Vec<String> {
    let rows = ctx.app.chat_history().get_messages(id).await.expect("read");
    rows.into_iter().map(|row| row.content).collect()
}

const ANSWERED: [(MessageRole, &str); 2] = [
    (User, "Plan a trip to Kyoto"),
    (Assistant, "Day 1: temples"),
];

#[tokio::test]
async fn retry_answers_a_saved_reply_again_on_a_new_branch_and_the_session_goes_on_there() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &ANSWERED).await;
    let id = session.id;

    let answer = go(&mut session, Ask::Retry).await;

    let branch = session.id;
    assert_ne!(branch, id);
    assert!(answer);
    assert_eq!(
        contents(&ctx, id).await,
        ["Plan a trip to Kyoto", "Day 1: temples"]
    );
    assert_eq!(contents(&ctx, branch).await, ["Plan a trip to Kyoto"]);
}

#[tokio::test]
async fn retry_answers_a_question_nothing_answers_where_it_is() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &[(User, "Plan a trip to Kyoto")]).await;
    let id = session.id;

    let answer = go(&mut session, Ask::Retry).await;

    assert_eq!(session.id, id);
    assert!(answer);
    assert_eq!(contents(&ctx, id).await, ["Plan a trip to Kyoto"]);
    assert_eq!(
        ctx.app
            .chat_history()
            .list_conversations()
            .await
            .expect("read")
            .len(),
        1
    );
}

#[tokio::test]
async fn an_edit_of_an_answered_question_branches_and_one_nothing_answers_is_made_in_place() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &ANSWERED).await;
    let id = session.id;

    let answer = go(&mut session, Ask::Edit("Plan a trip to Osaka".into())).await;
    let branch = session.id;
    assert!(answer);
    assert_ne!(branch, id);
    assert_eq!(contents(&ctx, branch).await, ["Plan a trip to Osaka"]);
    assert_eq!(
        contents(&ctx, id).await,
        ["Plan a trip to Kyoto", "Day 1: temples"]
    );

    let answer = go(&mut session, Ask::Edit("Plan a trip to Nara".into())).await;
    assert!(answer);
    assert_eq!(session.id, branch);
    assert_eq!(contents(&ctx, branch).await, ["Plan a trip to Nara"]);
}

#[tokio::test]
async fn branch_copies_the_chat_to_go_on_in_and_answers_nothing() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &ANSWERED).await;
    let id = session.id;

    let answer = go(&mut session, Ask::Branch).await;

    let branch = session.id;
    assert_ne!(branch, id);
    assert!(!answer);
    assert_eq!(contents(&ctx, branch).await, contents(&ctx, id).await);
}

#[tokio::test]
async fn a_change_that_cannot_be_made_leaves_the_session_where_it_was() {
    let dir = tempfile::tempdir().expect("a directory");
    let ctx = test_context(dir.path()).await;
    let mut session = chat(&ctx, &[(Assistant, "Hello! Ask me anything.")]).await;
    let id = session.id;

    for ask in [Ask::Retry, Ask::Edit("Hi".into()), Ask::Edit(String::new())] {
        assert!(!go(&mut session, ask).await);
    }
    assert_eq!(session.id, id);
    assert_eq!(
        ctx.app
            .chat_history()
            .list_conversations()
            .await
            .expect("read")
            .len(),
        1
    );
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
