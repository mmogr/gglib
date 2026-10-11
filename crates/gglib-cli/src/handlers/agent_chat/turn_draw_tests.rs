//! A message sent after `/draw`, through the loop `gglib chat` composes and
//! a llama-server that keeps each request: the closing request ends with
//! the drawing side's instruction, and the instruction is in no saved row
//! and not in the context the next message is sent with.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::domain::chat::MessageRole;
use gglib_core::ports::{ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers};
use gglib_runtime::{LoopGeneration, compose_agent_loop_with_sampling};
use serde_json::{Value, json};

use super::*;
use crate::bootstrap::test_context;
use crate::handlers::agent_chat::sight::sight_tests::read_request;
use crate::target::TurnModel;

/// One reply that calls the image tool, as llama-server streams a native
/// call.
const DRAWS: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",",
    "\"type\":\"function\",\"function\":{\"name\":\"builtin:generate_image\",",
    "\"arguments\":\"{\\\"prompt\\\":\\\"a fox\\\"}\"}}]}}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// One reply in words.
const WORDS: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"content\":\"The driver was not there.\"}}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// A llama-server on loopback that answers [`DRAWS`], then [`WORDS`], and
/// keeps each request's body.
fn llama() -> (u16, Arc<Mutex<Vec<Value>>>) {
    answering(&[DRAWS, WORDS])
}

/// A llama-server on loopback that answers one request with each of
/// `replies`, in order, and keeps each request's body.
fn answering(replies: &'static [&'static str]) -> (u16, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&bodies);
    std::thread::spawn(move || {
        for reply in replies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let (_, body) = read_request(&mut stream);
            kept.lock()
                .unwrap()
                .push(serde_json::from_str(&body).expect("a JSON body"));
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (port, bodies)
}

/// A driver that can draw and whose render fails, so the tool answers the
/// call with an error.
#[derive(Debug)]
struct DrawsNothing;

#[async_trait::async_trait]
impl ImageGenerationPort for DrawsNothing {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        Err(ImageError::Failed {
            message: "a test's driver draws nothing".to_owned(),
        })
    }

    async fn drawing(
        &self,
        _chosen: Option<u32>,
    ) -> gglib_core::contracts::http::images::DrawingAvailability {
        gglib_core::contracts::http::images::DrawingAvailability::with("sdxl")
    }
}

#[tokio::test]
async fn the_closing_instruction_reaches_the_model_and_no_saved_row_or_later_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (port, bodies) = llama();
    let draw = DrawSwitch::through(Arc::new(DrawsNothing));
    let agent = compose_agent_loop_with_sampling(
        format!("http://127.0.0.1:{port}"),
        reqwest::Client::new(),
        Some("qwen".to_owned()),
        ModelContext::passthrough(),
        Arc::clone(&ctx.mcp),
        None,
        None,
        None,
        SamplingLayers::default(),
        None,
        None,
        None,
        Some(gglib_core::retry::RetryPolicy::default()),
        None,
        ctx.app.attachments().store(),
        LoopGeneration {
            gate: None,
            drawing: draw.tool(&ctx),
        },
    );
    let made_by = TurnModel::here("qwen".to_owned(), None).made_by();
    let chat = Conversation::create(ctx.app.chat_history(), None, None, made_by).await;
    let chat = chat.expect("a conversation");
    draw.arm().await;
    assert!(draw.is_armed());

    let asked = vec![AgentMessage::user("draw a fox")];
    let config = AgentConfig::default();
    let after = send(&agent, asked, config, false, Some(&chat), true, &draw).await;

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2, "the call, then the words");
    let sent = bodies[1]["messages"].as_array().expect("messages");
    assert_eq!(sent[sent.len() - 2]["role"], "tool");
    let told = json!({ "role": "user", "content": gglib_mcp::DRAW_THEN_SAY_FAILED });
    assert_eq!(sent[sent.len() - 1], told);
    assert!(bodies[1].get("tools").is_none(), "{}", bodies[1]);

    let instructs = |text: &str| {
        text.contains(gglib_mcp::DRAW_THEN_SAY) || text.contains(gglib_mcp::DRAW_THEN_SAY_FAILED)
    };
    let context = serde_json::to_string(&after).expect("the next message's context");
    assert_eq!(after.len(), 4, "message, call, result, answer: {context}");
    assert!(!instructs(&context), "{context}");

    let rows = ctx.app.chat_history().get_messages(chat.id).await;
    let rows = rows.expect("read");
    let roles: Vec<MessageRole> = rows.iter().map(|row| row.role).collect();
    let (user, said) = (MessageRole::User, MessageRole::Assistant);
    assert_eq!(roles, [user, said, MessageRole::Tool, said]);
    assert_eq!(rows[0].content, "draw a fox");
    assert_eq!(rows[3].content, "The driver was not there.");
    for row in &rows {
        let metadata = row.metadata.as_ref().map(Value::to_string);
        assert!(!instructs(&row.content), "{}", row.content);
        assert!(!instructs(&metadata.unwrap_or_default()), "{row:?}");
    }
}

/// The message after a drawn picture, sent without `/draw`: the model is
/// told once, at the front, that the picture exists, beside whatever other
/// tools the session offers. The note is in no saved row and not in the
/// context the message after it is sent with.
#[tokio::test]
async fn the_message_after_a_drawn_picture_tells_the_model_and_no_row_or_later_context() {
    use gglib_core::domain::agent::{AssistantContent, ToolCall};
    use gglib_core::request_pipeline::{DRAW_TOOL_NAME, DRAWN_NOTE};

    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let (port, bodies) = answering(&[WORDS]);
    let draw = DrawSwitch::through(Arc::new(DrawsNothing));
    let agent = compose_agent_loop_with_sampling(
        format!("http://127.0.0.1:{port}"),
        reqwest::Client::new(),
        Some("qwen".to_owned()),
        ModelContext::passthrough(),
        Arc::clone(&ctx.mcp),
        None,
        None,
        None,
        SamplingLayers::default(),
        None,
        None,
        None,
        Some(gglib_core::retry::RetryPolicy::default()),
        None,
        ctx.app.attachments().store(),
        LoopGeneration {
            gate: None,
            drawing: draw.tool(&ctx),
        },
    );
    let made_by = TurnModel::here("qwen".to_owned(), None).made_by();
    let chat = Conversation::create(ctx.app.chat_history(), None, None, made_by).await;
    let chat = chat.expect("a conversation");
    assert!(!draw.is_armed());

    let call = ToolCall {
        id: "c1".to_owned(),
        name: DRAW_TOOL_NAME.to_owned(),
        arguments: json!({ "prompt": "a fox" }),
    };
    let answer = "Drew 1 image, 1024x1024 PNG, with flux in 76 s; the user can see it, you cannot.";
    let asked = vec![
        AgentMessage::user("draw a fox"),
        AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![call],
            },
        },
        AgentMessage::Tool {
            tool_call_id: "c1".to_owned(),
            content: answer.to_owned(),
        },
        AgentMessage::Assistant {
            content: AssistantContent {
                text: Some("I asked for a red fox.".to_owned()),
                tool_calls: Vec::new(),
            },
        },
        AgentMessage::user("thanks"),
    ];
    let config = AgentConfig::default();
    let after = send(&agent, asked, config, false, Some(&chat), true, &draw).await;

    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 1, "one request");
    let note = json!({ "role": "system", "content": DRAWN_NOTE });
    assert_eq!(bodies[0]["messages"][0], note);
    assert_eq!(bodies[0].to_string().matches(DRAWN_NOTE).count(), 1);
    let offered = bodies[0]["tools"].as_array().cloned().unwrap_or_default();
    let draws = |tool: &Value| tool["function"]["name"] == DRAW_TOOL_NAME;
    assert!(!offered.iter().any(draws), "{}", bodies[0]);

    let context = serde_json::to_string(&after).expect("the next message's context");
    assert_eq!(after.len(), 6, "the five sent and the answer: {context}");
    assert!(!context.contains(DRAWN_NOTE), "{context}");

    let rows = ctx.app.chat_history().get_messages(chat.id).await;
    let rows = rows.expect("read");
    assert_eq!(rows.len(), 2, "the message and the answer: {rows:?}");
    for row in &rows {
        let metadata = row.metadata.as_ref().map(Value::to_string);
        assert!(!row.content.contains(DRAWN_NOTE), "{}", row.content);
        assert!(
            !metadata.unwrap_or_default().contains(DRAWN_NOTE),
            "{row:?}"
        );
    }
}
