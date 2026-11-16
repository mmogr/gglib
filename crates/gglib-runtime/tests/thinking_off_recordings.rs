//! A turn with Thinking off, sent by the completion adapter to a stand-in
//! llama-server that answers with what the pinned llama-server answered.
//!
//! # The recordings
//!
//! Recorded 2026-11-16 on a Windows 11 x64 VM, on the CPU, from llama-server
//! b12110 (`b12110-cd959bae9`, the pin) serving Qwen3-4B `Q4_K_M`
//! (`Qwen3-4B-Q4_K_M.gguf`), launched as gglib launches a `reasoning`-tagged
//! model:
//!
//! ```text
//! llama-server -m C:\gglib-test\models\Qwen3-4B-Q4_K_M.gguf --host 127.0.0.1 --port 9123
//!   --metrics --parallel 1 -c 8192 --jinja --reasoning-format deepseek
//!   --cache-ram 0 --cache-type-k q8_0 --cache-type-v q8_0
//! ```
//!
//! Each file is the reply to one request, posted to llama-server's own port,
//! byte for byte as it came; every frame's `model` names the GGUF in the
//! VM's test folder. The requests, exactly:
//!
//! - `fixtures/llama_b12110_qwen3_budget0.sse`, what gglib sent for Thinking
//!   off before the kwarg:
//!   `{"messages":[{"role":"user","content":"What is the capital of France?"}],"temperature":0,"seed":1,"max_tokens":400,"stream":true,"reasoning_budget_tokens":0}`.
//!   llama-server renders the prompt with thinking on and its sampler closes
//!   the thinking at once; the model then reasons in plain text, writes its
//!   own `</think>` and answers. No frame carries `reasoning_content`, so all
//!   of it is reply text.
//! - `fixtures/llama_b12110_qwen3_budget0_enable_thinking_false.sse`, what
//!   gglib sends now: the same, with
//!   `"chat_template_kwargs":{"enable_thinking":false}` after the budget. The
//!   template opens and closes an empty thinking block itself, and the reply
//!   is the answer alone.
//!
//! The same request with the kwarg and no budget gave the same 35 characters;
//! with neither, 894 characters of `reasoning_content`, then a clean answer.

use std::sync::{Arc, Mutex};

use gglib_agent::{CollectedResponse, collect_stream};
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::ports::LlmCompletionPort as _;
use gglib_runtime::LlmCompletionAdapter;
use serde_json::Value;
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// Thinking off with only the budget: the reasoning is the reply.
const BUDGET_ONLY: &[u8] = include_bytes!("fixtures/llama_b12110_qwen3_budget0.sse");

/// Thinking off with the budget and `enable_thinking: false`.
const WITH_THE_KWARG: &[u8] =
    include_bytes!("fixtures/llama_b12110_qwen3_budget0_enable_thinking_false.sse");

/// The recorded question.
const QUESTION: &str = "What is the capital of France?";

/// What a stand-in was posted, once it has been.
type Seen = Arc<Mutex<Option<Value>>>;

/// A llama-server on loopback that reads one request whole, keeps its body,
/// and answers with `recording`.
async fn llama(recording: &'static [u8]) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let base = format!("http://{}", listener.local_addr().expect("its address"));
    let seen = Seen::default();
    let keep = Arc::clone(&seen);
    tokio::spawn(async move {
        let Ok((request, mut socket)) = gglib_test_http::accept(&listener).await else {
            return;
        };
        let body: Value = serde_json::from_slice(&request.body).expect("a JSON body");
        *keep.lock().expect("the lock") = Some(body);
        let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                    connection: close\r\n\r\n";
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(recording).await;
        let _ = socket.shutdown().await;
    });
    (base, seen)
}

/// One turn with Thinking off, as a run asks for it (`reasoning_budget_tokens:
/// 0`, the person's own choice), sent straight to a llama-server that answers
/// with `recording`: what the agent loop collected, and the body it sent.
async fn thinking_off_turn(recording: &'static [u8]) -> (CollectedResponse, Value) {
    let (base, seen) = llama(recording).await;
    let adapter = LlmCompletionAdapter::new(base, Some("Qwen3-4B".to_owned())).with_sampling(Some(
        InferenceConfig {
            reasoning_budget_tokens: Some(0),
            ..InferenceConfig::default()
        },
    ));
    let stream = adapter
        .chat_stream(&[AgentMessage::user(QUESTION)], &[])
        .await
        .expect("the request is answered");
    // Room for every delta: nothing reads the channel while the collector runs.
    let (tx, _rx) = mpsc::channel(4096);
    let collected = collect_stream(stream, &tx).await.expect("a whole reply");
    let sent = seen.lock().expect("the lock").clone().expect("a request");
    (collected, sent)
}

/// The recording's own reply text and reasoning, joined from its frames
/// without gglib's parser: what gglib must pass on unchanged.
fn recorded(recording: &[u8]) -> (String, String) {
    let text = std::str::from_utf8(recording).expect("utf-8");
    let (mut content, mut reasoning) = (String::new(), String::new());
    for data in text.lines().filter_map(|l| l.strip_prefix("data: ")) {
        let Ok(frame) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let delta = &frame["choices"][0]["delta"];
        content.push_str(delta["content"].as_str().unwrap_or_default());
        reasoning.push_str(delta["reasoning_content"].as_str().unwrap_or_default());
    }
    (content, reasoning)
}

/// The defect, as the user saw it: with the budget alone the model's
/// reasoning arrives as reply text, a stray `</think>` and all, and gglib
/// passes it on as it came. No strip papers over it (ADR 0022).
#[tokio::test]
async fn with_the_budget_alone_the_reasoning_is_the_reply_and_gglib_passes_it_on() {
    let (reply, _) = thinking_off_turn(BUDGET_ONLY).await;

    let (content, reasoning) = recorded(BUDGET_ONLY);
    assert_eq!(reply.content, content, "gglib adds and removes nothing");
    assert_eq!(reply.content.chars().count(), 951);
    assert!(reasoning.is_empty() && reply.reasoning_content.is_empty());
    assert!(
        reply
            .content
            .starts_with("Okay, the user is asking for the capital of France."),
        "{}",
        reply.content
    );
    let close = reply
        .content
        .find("</think>")
        .expect("the model's own close");
    assert_eq!(close, 798);
    assert!(!reply.content.contains("<think>"));
    assert!(reply.content[close..].contains("**Paris**"));
    assert_eq!(reply.finish_reason.as_deref(), Some("stop"));
}

/// The fix, as the user sees it: with `enable_thinking: false` beside the
/// budget, the reply is the answer and nothing else.
#[tokio::test]
async fn with_the_kwarg_beside_the_budget_the_reply_is_the_answer_alone() {
    let (reply, _) = thinking_off_turn(WITH_THE_KWARG).await;

    assert_eq!(reply.content, "The capital of France is **Paris**.");
    assert!(reply.reasoning_content.is_empty());
    assert_eq!(reply.finish_reason.as_deref(), Some("stop"));
}

/// What the adapter sends for that turn: the budget, and the template told
/// that thinking is off.
#[tokio::test]
async fn a_thinking_off_turn_tells_the_template_beside_the_budget() {
    let (_, sent) = thinking_off_turn(WITH_THE_KWARG).await;

    assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
    assert_eq!(
        sent["chat_template_kwargs"]["enable_thinking"],
        Value::Bool(false),
        "{sent}"
    );
}
