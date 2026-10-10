//! A run whose first reply must call one tool, as the wire carries it: the
//! real agent loop over this adapter, against a llama-server that keeps
//! each request's body and answers from a script.
//!
//! The first request offers that tool alone and demands a call; every later
//! one offers the run's whole list and leaves the choice to the model. A
//! run that demands nothing sends what it always did.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{
    AgentConfig, AgentEvent, AgentMessage, FirstCall, ToolCall, ToolDefinition, ToolResult,
};
use gglib_core::domain::{DialectSpec, ModelCapabilities};
use gglib_core::normalize::tags::FORMAT_QWEN_XML;
use gglib_core::ports::{AgentError, ToolExecutorPort};
use gglib_core::request_pipeline::ModelContext;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::LlmCompletionAdapter;
use super::images::ImageUrls;

const DRAW: &str = "builtin:generate_image";
const READ: &str = "3:read_file";
const NOT_ASKED: &str = "the model did not ask for the picture";

/// One reply that calls `DRAW`, as llama-server streams a native call.
fn calls_draw() -> String {
    let call = json!({"choices": [{"index": 0, "delta": {"tool_calls": [{
        "index": 0, "id": "c1", "type": "function",
        "function": {"name": DRAW, "arguments": "{\"prompt\":\"a red fox\"}"},
    }]}}]});
    let end = json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]});
    format!("data: {call}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

/// One reply that only says `text`.
fn says(text: &str) -> String {
    let delta = json!({"choices": [{"index": 0, "delta": {"content": text}}]});
    let end = json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]});
    format!("data: {delta}\n\ndata: {end}\n\ndata: [DONE]\n\n")
}

/// A llama-server on loopback that reads each request whole, keeps its
/// body, and answers the next reply of its script.
async fn llama(script: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&bodies);
    tokio::spawn(async move {
        for reply in script {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let body = read_body(&mut socket).await;
            kept.lock().unwrap().push(body);
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                        connection: close\r\n\r\n";
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    (base, bodies)
}

/// Read one request's head and the body its `content-length` names.
async fn read_body(socket: &mut tokio::net::TcpStream) -> Value {
    let mut read = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0, "the request ended early");
        read.extend_from_slice(&chunk[..n]);
        let Some(start) = read.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&read[..start]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .map_or(0, |value| value.trim().parse::<usize>().unwrap());
        if read.len() >= start + 4 + length {
            return serde_json::from_slice(&read[start + 4..start + 4 + length]).unwrap();
        }
    }
}

/// Lists the image tool and an MCP server's, and counts what it ran.
#[derive(Default)]
struct TwoTools {
    ran: Mutex<Vec<String>>,
}

#[async_trait]
impl ToolExecutorPort for TwoTools {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new(DRAW), ToolDefinition::new(READ)]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        self.ran.lock().unwrap().push(call.name.clone());
        Ok(ToolResult::text(
            call.id.clone(),
            "Drew 1 image.".to_owned(),
            true,
        ))
    }
}

fn draw_first() -> FirstCall {
    FirstCall {
        tool: DRAW.to_owned(),
        if_missing: NOT_ASKED.to_owned(),
    }
}

/// Run one turn against `script`, with `first_call` as the run's config
/// has it: how it ended, the events it sent, each request body, and the
/// tools it ran.
async fn run(
    script: Vec<String>,
    first_call: Option<FirstCall>,
) -> (
    Result<String, AgentError>,
    Vec<AgentEvent>,
    Vec<Value>,
    Vec<String>,
) {
    let (base, bodies) = llama(script).await;
    let tools = Arc::new(TwoTools::default());
    let adapter = LlmCompletionAdapter::new(&base, Some("m".to_owned()));
    let executor = Arc::clone(&tools) as Arc<dyn ToolExecutorPort>;
    let agent = AgentLoop::build(Arc::new(adapter), executor, None);
    let mut config = AgentConfig::default();
    config.first_call = first_call;
    let (tx, mut rx) = mpsc::channel(256);

    let ended = agent
        .run(vec![AgentMessage::user("draw a red fox")], config, tx)
        .await
        .map(|output| output.answer);

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    let bodies = bodies.lock().unwrap().clone();
    let ran = tools.ran.lock().unwrap().clone();
    (ended, events, bodies, ran)
}

/// The names of the tools a request body offers.
fn offered(body: &Value) -> Vec<&str> {
    body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["function"]["name"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// The run's first request offers exactly the demanded tool and demands a
/// call; the second offers every tool and leaves the choice to the model.
#[tokio::test]
async fn the_first_request_offers_only_the_demanded_tool_and_requires_a_call() {
    let script = vec![calls_draw(), says("A red fox in the snow.")];

    let (ended, _events, bodies, ran) = run(script, Some(draw_first())).await;

    assert_eq!(ended.unwrap(), "A red fox in the snow.");
    assert_eq!(bodies.len(), 2, "two calls to the model");
    assert_eq!(offered(&bodies[0]), [DRAW]);
    assert_eq!(bodies[0]["tool_choice"], "required");
    assert_eq!(offered(&bodies[1]), [DRAW, READ]);
    assert_eq!(bodies[1]["tool_choice"], "auto");
    assert_eq!(ran, [DRAW]);
}

/// A run that demands nothing sends what it always did: every tool and the
/// model's own choice, on its first request as on any other.
#[tokio::test]
async fn a_run_that_demands_no_call_offers_every_tool_and_requires_nothing() {
    let script = vec![calls_draw(), says("Done.")];

    let (ended, _events, bodies, _ran) = run(script, None).await;

    assert_eq!(ended.unwrap(), "Done.");
    assert_eq!(bodies.len(), 2);
    for body in &bodies {
        assert_eq!(offered(body), [DRAW, READ]);
        assert_eq!(body["tool_choice"], "auto");
    }
}

/// A first reply that calls nothing, from a server that ignored the demand,
/// fails the run with the composer's words as its last `error` event. No
/// second request is made and the model's text is not the run's answer.
#[tokio::test]
async fn a_first_reply_with_no_call_fails_the_run_in_the_composers_words() {
    let fenced = "```json\n{\"name\":\"builtin:generate_image\"}\n```";

    let (ended, events, bodies, ran) = run(vec![says(fenced)], Some(draw_first())).await;

    match ended {
        Err(AgentError::FirstCallMissing { message }) => assert_eq!(message, NOT_ASKED),
        other => panic!("expected the missing first call, got {other:?}"),
    }
    assert_eq!(bodies.len(), 1, "the run stopped at its first reply");
    assert!(ran.is_empty());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::FinalAnswer { .. })),
        "the text was no answer"
    );
    let last_error = events.iter().rev().find_map(|event| match event {
        AgentEvent::Error { message } => Some(message.as_str()),
        _ => None,
    });
    assert_eq!(last_error, Some(NOT_ASKED));
}

/// The body a demanded call is shaped into differs from the one that
/// demands nothing only in `tool_choice`, which is `auto` unless demanded.
#[test]
fn only_tool_choice_tells_a_demanded_body_from_any_other() {
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()));
    let tools = [ToolDefinition::new(DRAW)];
    let messages = [AgentMessage::user("draw a red fox")];
    let shaped = |must_call| {
        adapter
            .shaped_body(&messages, &tools, &ImageUrls::default(), must_call)
            .unwrap()
    };

    let (free, mut demanded) = (shaped(false), shaped(true));

    assert_eq!(free["tool_choice"], "auto");
    assert_eq!(demanded["tool_choice"], "required");
    demanded["tool_choice"] = json!("auto");
    assert_eq!(free, demanded);
    let none = adapter
        .shaped_body(&messages, &[], &ImageUrls::default(), true)
        .unwrap();
    assert!(
        none.get("tool_choice").is_none(),
        "no tools, nothing to demand"
    );
}

/// For a model that writes its calls in a dialect, the demand becomes the
/// pipeline's grammar for a call of that one tool, whose qualified name,
/// colon included, the grammar can spell.
#[test]
fn a_dialect_model_gets_a_grammar_for_the_one_demanded_tool() {
    let qwen = ModelContext {
        tags: vec![FORMAT_QWEN_XML.to_owned()],
        dialect: Some(DialectSpec::qwen_xml()),
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS
            | ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_model_context(qwen);
    let tools = [ToolDefinition::new(DRAW)];
    let messages = [AgentMessage::user("draw a red fox")];

    let body = adapter
        .shaped_body(&messages, &tools, &ImageUrls::default(), true)
        .unwrap();

    let grammar = body["grammar"].as_str().expect("a grammar was installed");
    assert!(grammar.contains(DRAW), "{grammar}");
    assert_eq!(
        body["tool_choice"], "none",
        "the demand lives in the grammar"
    );
    let free = adapter
        .shaped_body(&messages, &tools, &ImageUrls::default(), false)
        .unwrap();
    assert!(
        free.get("grammar").is_none(),
        "nothing demanded, no grammar"
    );
}
