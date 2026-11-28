//! Whether a composed loop keeps each step's reasoning, read from the model
//! context it is given: only a model whose stored capabilities say its
//! template keeps reasoning has it sent back, as `reasoning_content` on the
//! step's assistant message of every later request in the run.

use std::sync::Mutex;

use async_trait::async_trait;
use gglib_core::domain::agent::{AgentConfig, AgentMessage};
use gglib_core::domain::mcp::{McpServer, NewMcpServer};
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentInfo, TemplateCaps};
use gglib_core::ports::{AttachmentError, McpRepositoryError, McpServerRepository};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::*;

const THOUGHT: &str = "The bug is in parse(); read src/parse.rs first.";

/// No MCP server is configured.
struct NoServers;

#[async_trait]
impl McpServerRepository for NoServers {
    async fn insert(&self, _: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        unreachable!("the loop adds no server")
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.to_owned()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(Vec::new())
    }
    async fn update(&self, _: &McpServer) -> Result<(), McpRepositoryError> {
        unreachable!("the loop changes no server")
    }
    async fn delete(&self, _: i64) -> Result<(), McpRepositoryError> {
        unreachable!("the loop removes no server")
    }
    async fn update_last_connected(&self, _: i64) -> Result<(), McpRepositoryError> {
        unreachable!("no server is connected")
    }
    async fn set_allow_remote(&self, _: i64, _: bool) -> Result<(), McpRepositoryError> {
        unreachable!("the loop changes no server")
    }
}

/// A store with no images in it.
struct NoImages;

#[async_trait]
impl AttachmentStore for NoImages {
    async fn put(&self, _: &AttachmentInfo, _: &[u8]) -> Result<(), AttachmentError> {
        unreachable!("no tool here makes an image")
    }
    async fn info(&self, _: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        Ok(None)
    }
    async fn size(&self, _: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        Ok(None)
    }
    async fn blob(&self, _: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        Ok(None)
    }
    async fn ids_starting_with(&self, _: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        Ok(Vec::new())
    }
}

/// A reply that thinks [`THOUGHT`] and calls `search`, as llama-server
/// streams it.
fn thinks_and_calls() -> String {
    let thought = json!({"choices": [{"index": 0, "delta": {"reasoning_content": THOUGHT}}]});
    let call = json!({"choices": [{"index": 0, "delta": {"tool_calls": [{
        "index": 0, "id": "c1", "type": "function",
        "function": {"name": "search", "arguments": "{}"},
    }]}}]});
    let end = json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]});
    format!("data: {thought}\n\ndata: {call}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

/// A reply that only says `text`.
fn says(text: &str) -> String {
    let delta = json!({"choices": [{"index": 0, "delta": {"content": text}}]});
    let end = json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]});
    format!("data: {delta}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

/// An endpoint on loopback that reads each request whole, keeps its body,
/// and answers the next reply of `script`.
async fn endpoint(script: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&bodies);
    tokio::spawn(async move {
        for reply in script {
            let Ok((request, mut socket)) = gglib_test_http::accept(&listener).await else {
                return;
            };
            kept.lock()
                .unwrap()
                .push(serde_json::from_slice(&request.body).unwrap());
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                        connection: close\r\n\r\n";
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    (base, bodies)
}

/// A model context whose stored caps answer `keeps` for preserving
/// reasoning, `None` being caps that do not carry the field.
fn caps(keeps: Option<bool>) -> ModelContext {
    ModelContext {
        template_caps: Some(TemplateCaps {
            supports_preserve_reasoning: keeps,
            ..TemplateCaps::default()
        }),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

/// Run two steps of a loop composed with `context` against a scripted
/// endpoint (a step that thinks and calls a tool, then an answer), and
/// answer each request's body.
async fn two_steps(context: ModelContext) -> Vec<Value> {
    let (base, bodies) = endpoint(vec![thinks_and_calls(), says("Fixed.")]).await;
    let door = Door {
        root_url: base.clone(),
        key: None,
        name: None,
    };
    let agent = compose_agent_loop(
        base,
        Client::new(),
        Some("m".to_owned()),
        context,
        Arc::new(McpService::new(Arc::new(NoServers))),
        None,
        None,
        None,
        None,
        None,
        SamplingLayers::default(),
        door,
        Arc::new(NoImages),
        None,
    );
    let (tx, mut rx) = mpsc::channel(256);
    let ended = agent
        .run(
            vec![AgentMessage::user("find the bug")],
            AgentConfig::default(),
            tx,
        )
        .await;
    while rx.recv().await.is_some() {}
    assert_eq!(ended.unwrap().answer, "Fixed.");
    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2, "two requests");
    bodies
}

/// The assistant message of a request body: the one there is.
fn the_assistant(body: &Value) -> &Value {
    let assistants: Vec<&Value> = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "assistant")
        .collect();
    let [assistant] = assistants[..] else {
        panic!("one assistant message: {body}");
    };
    assistant
}

/// A model whose template keeps reasoning: the second request's assistant
/// message carries the first step's reasoning beside its call.
#[tokio::test]
async fn a_model_that_keeps_reasoning_is_sent_the_first_steps_reasoning() {
    let bodies = two_steps(caps(Some(true))).await;

    let step = the_assistant(&bodies[1]);
    assert_eq!(step["reasoning_content"], json!(THOUGHT));
    assert_eq!(step["tool_calls"][0]["id"], json!("c1"));
}

/// Any other model is sent none: its caps say no, do not carry the field,
/// are missing (never launched), or the context is a passthrough one (a
/// model on another machine).
#[tokio::test]
async fn any_other_model_is_sent_no_reasoning() {
    let others = [
        caps(Some(false)),
        caps(None),
        ModelContext {
            catalog_resolved: true,
            ..ModelContext::passthrough()
        },
        ModelContext::passthrough(),
    ];
    for context in others {
        let bodies = two_steps(context.clone()).await;

        let step = the_assistant(&bodies[1]);
        assert!(
            step.get("reasoning_content").is_none(),
            "{context:?}: {step}"
        );
        assert_eq!(step["tool_calls"][0]["id"], json!("c1"));
    }
}

/// The gate itself, on each answer the stored caps can give.
#[test]
fn only_a_yes_keeps_reasoning() {
    assert!(keeps_reasoning(&caps(Some(true))));
    assert!(!keeps_reasoning(&caps(Some(false))));
    assert!(!keeps_reasoning(&caps(None)));
    assert!(!keeps_reasoning(&ModelContext::passthrough()));
}
