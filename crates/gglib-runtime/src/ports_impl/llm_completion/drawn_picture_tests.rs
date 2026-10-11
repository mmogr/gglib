//! What a request shows its model of the pictures the drawing tool made:
//! the rule is `gglib_core::request_pipeline::shown_picture`'s, and these
//! are the bytes it becomes, and what never ends a request.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentMessage, AssistantContent, ToolCall};
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::ports::{AttachmentError, AttachmentStore, LlmCompletionPort as _};
use gglib_core::request_pipeline::{
    DRAW_THEN_SAY, DRAW_TOOL_NAME, DRAWN_NOTE, DRAWN_PICTURE_LINE, MAX_IMAGE_BYTES,
    MAX_REQUEST_IMAGE_BYTES, ModelContext,
};
use gglib_core::retry::RetryPolicy;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::super::LlmCompletionAdapter;
use super::super::body::build_chat_body;
use super::far_images_tests::{capture_one, head_end};
use super::images_tests::{Kept, user};
use super::resolve;

const DREW: &str = "Drew 1 image, 1024x1024 PNG, with m in 9 s; the user can see it, you cannot.";

fn call(id: &str, tool: &str) -> AgentMessage {
    AgentMessage::Assistant {
        content: AssistantContent {
            text: None,
            tool_calls: vec![ToolCall {
                id: id.to_owned(),
                name: tool.to_owned(),
                arguments: json!({ "prompt": "a fox" }),
            }],
        },
    }
}

fn made(call_id: &str, image: &AttachmentId) -> AgentMessage {
    AgentMessage::Tool {
        tool_call_id: call_id.to_owned(),
        content: DREW.to_owned(),
        images: vec![image.clone()],
    }
}

fn said(text: &str) -> AgentMessage {
    AgentMessage::Assistant {
        content: AssistantContent {
            text: Some(text.to_owned()),
            tool_calls: Vec::new(),
        },
    }
}

/// A chat in which `picture` was drawn, up to its closing reply.
fn one_drawn(picture: &AttachmentId) -> Vec<AgentMessage> {
    vec![
        user("draw a fox", &[]),
        call("c1", DRAW_TOOL_NAME),
        made("c1", picture),
        said("I asked for a fox."),
    ]
}

/// The `messages` of the body built for `messages`, for a model that reads
/// images or one that does not.
async fn wire(store: &Arc<dyn AttachmentStore>, messages: &[AgentMessage], sees: bool) -> Value {
    let images = resolve(Some(store), messages, sees).await.unwrap();
    build_chat_body("m", messages, &[], None, &images)["messages"].take()
}

fn image_part(url: &str) -> Value {
    json!({ "type": "image_url", "image_url": { "url": url } })
}

/// A PNG's signature and `IHDR`, for `side` by `side`: enough for its size
/// to be read, and so for the image to be priced.
fn png(side: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(side.to_be_bytes());
    bytes.extend(side.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// The whole of what a model that reads images is sent for a chat with one
/// drawn picture: every message as it was, but the one after the picture,
/// which starts with the line and the picture.
#[tokio::test]
async fn the_message_after_a_drawn_picture_starts_with_the_line_and_the_picture() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("what is in the corner?", &[]));

    assert_eq!(
        wire(&store, &chat, true).await,
        json!([
            { "role": "user", "content": "draw a fox" },
            { "role": "assistant", "tool_calls": [{
                "id": "c1",
                "type": "function",
                "function": { "name": DRAW_TOOL_NAME, "arguments": "{\"prompt\":\"a fox\"}" },
            }] },
            { "role": "tool", "tool_call_id": "c1", "content": DREW },
            { "role": "assistant", "content": "I asked for a fox." },
            { "role": "user", "content": [
                { "type": "text", "text": DRAWN_PICTURE_LINE },
                image_part("data:image/png;base64,Zm94"),
                { "type": "text", "text": "what is in the corner?" },
            ] },
        ])
    );
}

/// A model that reads no images is sent the chat exactly as a chat whose
/// tool message names no image is: no part, no line, and no refusal.
#[tokio::test]
async fn a_model_that_reads_no_images_is_sent_the_chat_without_the_picture() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("what is in the corner?", &[]));
    let mut bare = chat.clone();
    bare[2] = AgentMessage::tool("c1", DREW);

    let sent = wire(&store, &chat, false).await;

    assert_eq!(sent, wire(&store, &bare, false).await);
    assert_eq!(
        sent[4],
        json!({ "role": "user", "content": "what is in the corner?" })
    );
    assert!(!sent.to_string().contains("image_url"));
}

/// As the chat goes on, what was sent up to and including the message that
/// carries the picture is sent again byte for byte.
#[tokio::test]
async fn the_picture_stays_where_it_was_on_every_later_message() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("what is in the corner?", &[]));
    let first = wire(&store, &chat, true).await;

    chat.push(said("A tree."));
    chat.push(user("and the sky?", &[]));
    let second = wire(&store, &chat, true).await;
    chat.push(said("Blue."));
    chat.push(user("thanks", &[]));
    let third = wire(&store, &chat, true).await;

    let first = first.as_array().unwrap();
    assert_eq!(first.len(), 5);
    for later in [&second, &third] {
        let later = later.as_array().unwrap();
        assert_eq!(
            serde_json::to_string(&later[..5]).unwrap(),
            serde_json::to_string(first).unwrap()
        );
        for message in &later[5..] {
            assert!(!message.to_string().contains("image_url"), "{message}");
        }
    }
    assert_eq!(third[8], json!({ "role": "user", "content": "thanks" }));
}

/// Of two pictures only the newer is sent, on the message after it; the
/// message that carried the older one is its bare text again.
#[tokio::test]
async fn only_the_newer_of_two_pictures_is_sent() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let blue = kept.keep("image/png", b"blue fox".to_vec());
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("draw it bluer", &[]));
    chat.push(call("c2", DRAW_TOOL_NAME));
    chat.push(made("c2", &blue));
    chat.push(said("I asked for a blue fox."));
    chat.push(user("which is better?", &[]));

    let sent = wire(&store, &chat, true).await;

    assert_eq!(
        sent[4],
        json!({ "role": "user", "content": "draw it bluer" })
    );
    assert_eq!(
        sent[8]["content"],
        json!([
            { "type": "text", "text": DRAWN_PICTURE_LINE },
            image_part("data:image/png;base64,Ymx1ZSBmb3g="),
            { "type": "text", "text": "which is better?" },
        ])
    );
    let whole = sent.to_string();
    assert_eq!(
        whole.matches("image_url\"").count(),
        2,
        "a part's type and its key"
    );
    assert!(!whole.contains("base64,Zm94"), "the older picture was sent");
}

/// Until a message follows the picture nothing is added: not on the run's
/// own closing request, which ends with the composer's instruction.
#[tokio::test]
async fn a_picture_no_message_follows_adds_nothing() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let store = kept.store();
    let chat = one_drawn(&fox);
    let mut closing = chat[..3].to_vec();
    closing.push(user(DRAW_THEN_SAY, &[]));

    for messages in [&chat, &closing] {
        let sent = wire(&store, messages, true).await.to_string();
        assert!(!sent.contains("image_url"), "{sent}");
        assert!(!sent.contains(DRAWN_PICTURE_LINE), "{sent}");
    }
}

/// A message that already carries the person's own images keeps them, in
/// their order, after its text; the picture and its line come first.
#[tokio::test]
async fn the_persons_own_images_stay_and_the_picture_goes_before_them() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let photo = kept.keep("image/jpeg", vec![0xFF, 0xD8, 0xFF, 0x00]);
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("like this one?", &[&photo]));
    chat.push(said("Close."));
    chat.push(user("", &[&photo]));

    let sent = wire(&store, &chat, true).await;

    assert_eq!(
        sent[4]["content"],
        json!([
            { "type": "text", "text": DRAWN_PICTURE_LINE },
            image_part("data:image/png;base64,Zm94"),
            { "type": "text", "text": "like this one?" },
            image_part("data:image/jpeg;base64,/9j/AA=="),
        ])
    );
    assert_eq!(
        sent[6]["content"],
        json!([image_part("data:image/jpeg;base64,/9j/AA==")])
    );
}

/// Another tool's image is sent to no model, on no message.
#[tokio::test]
async fn another_tools_image_is_never_sent() {
    let mut kept = Kept::default();
    let shot = kept.keep("image/png", b"shot".to_vec());
    let store = kept.store();
    let chat = [
        user("look at the page", &[]),
        call("c1", "3:generate_image"),
        made("c1", &shot),
        user("what is it?", &[]),
    ];

    let sent = wire(&store, &chat, true).await;

    assert_eq!(sent[3], json!({ "role": "user", "content": "what is it?" }));
    assert!(!sent.to_string().contains("image_url"));
}

/// A store that fails every read.
struct Broken;

#[async_trait]
impl AttachmentStore for Broken {
    async fn put(&self, _info: &AttachmentInfo, _bytes: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("the adapter only reads")
    }

    async fn info(&self, _id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        unreachable!("the adapter reads bytes, not facts")
    }

    async fn size(&self, _id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        unreachable!("the adapter reads bytes, not sizes")
    }

    async fn blob(&self, _id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Err(AttachmentError::Storage("the disk is gone".to_owned()))
    }

    async fn ids_starting_with(&self, _: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        unreachable!("the adapter reads by id")
    }
}

/// A picture that is no longer stored, a store that fails, and no store at
/// all: each leaves the picture and its line out and the request whole. An
/// older picture is not sent in its place.
#[tokio::test]
async fn a_picture_that_cannot_be_read_is_left_out_and_the_request_goes() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", b"fox".to_vec());
    let swept = AttachmentId::of(b"swept");
    let holds_the_older: Arc<dyn AttachmentStore> = kept.store();
    let broken: Arc<dyn AttachmentStore> = Arc::new(Broken);
    let mut chat = one_drawn(&fox);
    chat.push(user("draw it bluer", &[]));
    chat.push(call("c2", DRAW_TOOL_NAME));
    chat.push(made("c2", &swept));
    chat.push(user("which is better?", &[]));
    let mut bare = chat.clone();
    bare[2] = AgentMessage::tool("c1", DREW);
    bare[6] = AgentMessage::tool("c2", DREW);
    let want = wire(&holds_the_older, &bare, true).await;
    assert!(!want.to_string().contains("image_url"));

    for store in [Some(&holds_the_older), Some(&broken), None] {
        let images = resolve(store, &chat, true).await.unwrap();
        let sent = build_chat_body("m", &chat, &[], None, &images)["messages"].take();
        assert_eq!(sent, want);
    }
}

/// The picture is counted against the request's 16 MiB with the person's
/// own images, and is the one left out when it does not fit: a request the
/// person's images alone would not end is not ended by it.
#[tokio::test]
async fn the_picture_counts_against_the_request_cap_and_is_left_out_over_it() {
    let mut kept = Kept::default();
    let fits = kept.keep("image/png", vec![7; 1024]);
    let one_too_many = kept.keep("image/png", vec![8; 1025]);
    let full = kept.keep("image/png", vec![1; MAX_IMAGE_BYTES]);
    let nearly = kept.keep("image/png", vec![2; MAX_IMAGE_BYTES - 1024]);
    let store = kept.store();
    assert_eq!(MAX_REQUEST_IMAGE_BYTES, 2 * MAX_IMAGE_BYTES);

    let chat_with = |picture: &AttachmentId| {
        let mut chat = one_drawn(picture);
        chat.push(user("and these?", &[&full, &nearly]));
        chat
    };

    let at_the_cap = wire(&store, &chat_with(&fits), true).await;
    let parts = at_the_cap[4]["content"].as_array().unwrap();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0]["text"], DRAWN_PICTURE_LINE);

    let over = wire(&store, &chat_with(&one_too_many), true).await;
    let parts = over[4]["content"].as_array().unwrap();
    assert_eq!(parts.len(), 3, "the text and the person's two images");
    assert_eq!(parts[0]["text"], "and these?");
}

fn seeing(context_length: u64) -> ModelContext {
    ModelContext {
        image_input: true,
        context_length: Some(context_length),
        ..ModelContext::passthrough()
    }
}

/// Accept one request for `adapter`'s `messages` and give back the body it
/// sent.
async fn sent_by(
    adapter: impl FnOnce(String) -> LlmCompletionAdapter,
    messages: &[AgentMessage],
) -> Value {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let adapter = adapter(base).with_retry_policy(RetryPolicy {
        max_attempts: 1,
        ..RetryPolicy::default()
    });
    let (sent, answered) = tokio::join!(capture_one(listener), async {
        adapter.chat_stream(messages, &[]).await.map(|_| ())
    });
    answered.unwrap();
    let start = head_end(sent.as_bytes()).unwrap();
    serde_json::from_str(&sent[start..]).unwrap()
}

/// The adapter itself, asked for a reply: it sends the picture for a model
/// whose context says it reads images and for no other, and the standing
/// note is at the front either way.
#[tokio::test]
async fn the_adapter_sends_the_picture_only_for_a_model_that_reads_images() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", png(1024));
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("what is in the corner?", &[]));

    let sees = sent_by(
        |base| {
            LlmCompletionAdapter::new(base, None)
                .with_attachments(Some(Arc::clone(&store)))
                .with_model_context(seeing(32_768))
        },
        &chat,
    )
    .await;
    let blind = sent_by(
        |base| LlmCompletionAdapter::new(base, None).with_attachments(Some(Arc::clone(&store))),
        &chat,
    )
    .await;

    for body in [&sees, &blind] {
        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": DRAWN_NOTE })
        );
        assert_eq!(body["messages"].as_array().unwrap().len(), 6);
    }
    let parts = sees["messages"][5]["content"].as_array().unwrap();
    assert_eq!(
        parts[0],
        json!({ "type": "text", "text": DRAWN_PICTURE_LINE })
    );
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
        parts[2],
        json!({ "type": "text", "text": "what is in the corner?" })
    );
    assert_eq!(blind["messages"][5]["content"], "what is in the corner?");
    assert!(!blind.to_string().contains("image_url"));
}

/// The picture is charged to the context budget where the request is
/// measured: a 1024 by 1024 picture is 1,024 tokens, so a chat that fits a
/// small context without it does not fit with it.
#[tokio::test]
async fn the_picture_is_charged_to_the_context_budget() {
    let mut kept = Kept::default();
    let fox = kept.keep("image/png", png(1024));
    let store = kept.store();
    let mut chat = one_drawn(&fox);
    chat.push(user("what is in the corner?", &[]));
    let adapter = |context_length| {
        LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
            .with_model_context(seeing(context_length))
    };

    let shown = resolve(Some(&store), &chat, true).await.unwrap();
    let hidden = resolve(Some(&store), &chat, false).await.unwrap();

    // 1,000 tokens hold the chat's text and the note, and not 1,024 more.
    assert!(
        adapter(1_000)
            .shaped_body(&chat, &[], &hidden, false)
            .is_ok()
    );
    let over = adapter(1_000).shaped_body(&chat, &[], &shown, false);
    assert!(over.is_err(), "{over:?}");
    assert!(
        adapter(2_500)
            .shaped_body(&chat, &[], &shown, false)
            .is_ok()
    );
}
