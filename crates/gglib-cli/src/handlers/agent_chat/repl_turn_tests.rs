//! What the REPL sends as a turn: a message typed, with the session's
//! folder, Thinking choice and Draw switch; and after `/retry`, `/edit` or
//! `/branch`, a turn that answers the chat the change leaves.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::contracts::http::images::DrawingAvailability;
use gglib_core::domain::branching::ChatChanged;
use gglib_core::domain::chat::MessageRole::{Assistant, User};
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewMessage};

use gglib_core::ports::{ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::*;
use crate::bootstrap::{CliContext, test_context};
use crate::daemon_client::STAND_IN_PORT;
use crate::handlers::agent_chat::images::images_tests::{attached, service};
use crate::handlers::agent_chat::turn::turn_stand_in::{Answer, handle, stand_in};

/// A daemon that can draw with `flux1-schnell`.
#[derive(Debug)]
struct CanDraw;

#[async_trait]
impl ImageGenerationPort for CanDraw {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        unreachable!("the session draws nothing itself")
    }

    async fn drawing(&self, _chosen: Option<u32>) -> DrawingAvailability {
        DrawingAvailability::with("flux1-schnell")
    }
}

/// A chat holding `rows`, as the session's conversation.
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

/// A session on `conversation`, its turns said with `folder` and
/// `thinking`, its Draw switch able to draw.
fn session<'a>(
    conversation: Conversation<'a>,
    images: TurnImages<'a>,
    folder: Option<&str>,
    thinking: Option<Thinking>,
) -> Session<'a> {
    Session {
        daemon: handle(),
        conversation,
        images,
        draw: DrawSwitch::through(Arc::new(CanDraw)),
        folder: folder.map(str::to_owned),
        thinking,
        shown: Shown {
            verbose: false,
            quiet: true,
        },
    }
}

/// A message typed is a turn on the session's chat that carries its words
/// and images, the folder and Thinking choice the session says, and, after
/// `/draw`, `draw`.
#[tokio::test]
async fn a_message_is_a_turn_with_the_sessions_folder_thinking_and_draw() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (attachments, _) = service();
    let (images, _) = attached(&attachments, &[], true).await.unwrap();
    let conversation = chat(&ctx, &[]).await;
    let id = conversation.id;
    let session = session(conversation, images, Some("/work"), Some(Thinking::Off));
    let image = AttachmentId::of(b"a picture");

    let plain = session.turn("hi".to_owned(), vec![image.clone()], false);
    session.draw.arm().await;
    let drawn = session.turn("a fox".to_owned(), Vec::new(), false);

    assert_eq!(
        plain,
        LocalTurn {
            conversation_id: id,
            content: "hi".to_owned(),
            images: vec![image],
            thinking: Some(Thinking::Off),
            answer_saved: false,
            draw: false,
            image_model: None,
            folder: Some("/work".to_owned()),
        }
    );
    assert!(drawn.draw);
}

/// `/retry` typed on an answered chat has the daemon branch it, moves the
/// session to the branch, and comes to a turn that answers that branch's
/// question, already saved, adding no message: `answer_saved` on the
/// branch, with nothing else said.
#[tokio::test]
async fn a_retry_typed_is_a_turn_that_answers_the_saved_question_on_its_branch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (attachments, _) = service();
    let (images, _) = attached(&attachments, &[], true).await.unwrap();
    let answered = [(User, "Plan a trip"), (Assistant, "Day 1: temples")];
    let mut session = session(chat(&ctx, &answered).await, images, None, None);
    let branched = ChatChanged {
        conversation_id: 42,
        forked: true,
        answer: true,
    };
    let (port, taken) = stand_in(Answer::Changes(Ok(branched)));

    let typed = STAND_IN_PORT.scope(port, session.typed("/retry")).await;
    let Typed::Turn(turn) = typed else {
        panic!("/retry on an answered chat is a turn: {typed:?}");
    };
    let answered = STAND_IN_PORT.scope(port, session.send(&turn)).await;

    assert_eq!(session.conversation.id, 42);
    assert!(answered);
    let put = taken
        .lock()
        .unwrap()
        .iter()
        .find(|(line, _)| line.starts_with("PUT "))
        .map(|(_, body)| body.clone())
        .expect("the turn was sent");
    assert_eq!(
        serde_json::from_str::<Value>(&put).unwrap(),
        json!({"conversation_id": 42, "content": "", "answer_saved": true})
    );
}

/// A message typed is a turn that adds it, and a command is no turn.
#[tokio::test]
async fn a_message_typed_is_a_turn_that_adds_it_and_a_command_is_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (attachments, _) = service();
    let (images, _) = attached(&attachments, &[], true).await.unwrap();
    let mut session = session(chat(&ctx, &[]).await, images, None, None);

    let Typed::Turn(turn) = session.typed("hello").await else {
        panic!("a message is a turn");
    };
    assert!(!turn.answer_saved);
    assert_eq!(turn.content, "hello");
    assert!(matches!(session.typed("/help").await, Typed::Done));
    assert!(matches!(session.typed("/quit").await, Typed::Quit));
}

/// Draw is for one message: once its turn is sent, refused or not, the
/// next turn does not draw.
#[tokio::test]
async fn a_draw_is_for_one_message_even_one_the_daemon_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (attachments, _) = service();
    let (images, _) = attached(&attachments, &[], true).await.unwrap();
    let session = session(chat(&ctx, &[]).await, images, None, None);
    let (port, _) = stand_in(Answer::Refuses(
        503,
        json!({"error": "nothing can draw here", "code": "drawing_unavailable"}),
    ));
    session.draw.arm().await;
    let turn = session.turn("a fox".to_owned(), Vec::new(), false);
    assert!(turn.draw);

    let answered = STAND_IN_PORT.scope(port, session.send(&turn)).await;

    assert!(!answered);
    assert!(!session.turn("again".to_owned(), Vec::new(), false).draw);
}
