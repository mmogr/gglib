//! A run's stream ends when llama-server goes silent, at the proxy's own
//! bound, and not while it keeps sending (#1212).

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{StreamExt as _, stream};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentMessage, LlmStreamEvent};
use gglib_core::ports::{AgentError, EmptyToolExecutor};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

use super::sse_events;
use crate::LlmCompletionAdapter;

/// Long enough that a busy runner does not cut a healthy stream, short
/// enough that a stall ends a test quickly.
const IDLE: Duration = Duration::from_millis(300);

/// No test here should take anywhere near this long; a mutation that removes
/// the bound fails here instead of hanging.
const PATIENCE: Duration = Duration::from_secs(10);

/// One SSE frame of answer text.
fn frame(text: &str) -> Vec<u8> {
    let delta = serde_json::json!({"choices": [{"index": 0, "delta": {"content": text}}]});
    format!("data: {delta}\n\n").into_bytes()
}

/// A llama-server that reads the request, answers with headers and one word,
/// then keeps the connection open in silence. Returns its base URL.
async fn wedged_after_one_word() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("bound");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = vec![0_u8; 64 * 1024];
        let _ = socket.read(&mut request).await;
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("headers");
        socket.write_all(&frame("Hel")).await.expect("a word");
        tokio::time::sleep(Duration::from_hours(1)).await;
    });
    format!("http://{addr}")
}

/// Through the real agent loop, over a socket, as a run drives the adapter:
/// the run fails at the bound, and its error names it.
#[tokio::test]
async fn a_run_whose_llama_server_goes_silent_ends_at_the_idle_bound() {
    let mut adapter = LlmCompletionAdapter::new(wedged_after_one_word().await, None);
    adapter.stream_idle_timeout = IDLE;
    let agent = AgentLoop::build(Arc::new(adapter), Arc::new(EmptyToolExecutor), None);
    let (tx, _rx) = tokio::sync::mpsc::channel(64);
    let user = AgentMessage::User {
        content: "hi".to_owned(),
    };

    let started = Instant::now();
    let run = agent.run(vec![user], AgentConfig::default(), tx);
    let ended = tokio::time::timeout(PATIENCE, run).await.expect("it ends");
    let waited = started.elapsed();

    let Err(AgentError::Internal(reason)) = ended else {
        panic!("the run fails: {ended:?}");
    };
    assert!(
        reason.contains(&format!("sent nothing for {IDLE:?}")),
        "{reason}"
    );
    assert!(
        waited >= IDLE && waited < IDLE * 10,
        "ended after {waited:?}"
    );
}

/// A slow answer is not a silent one: frames that keep coming at gaps under
/// the bound are all read, though the whole takes several times the bound.
#[tokio::test(start_paused = true)]
async fn a_stream_that_keeps_sending_is_not_cut() {
    let gap = IDLE / 3;
    let frames = (0..10)
        .map(|_| frame("x"))
        .chain([b"data: [DONE]\n\n".to_vec()]);
    let bytes = stream::iter(frames).then(move |bytes| async move {
        tokio::time::sleep(gap).await;
        Ok::<_, std::io::Error>(bytes)
    });

    let started = tokio::time::Instant::now();
    let events: Vec<_> = sse_events(bytes, IDLE).collect().await;

    assert!(started.elapsed() > IDLE * 3, "took {:?}", started.elapsed());
    assert!(events.iter().all(Result::is_ok), "{events:?}");
    let words = events
        .iter()
        .filter(|e| matches!(e, Ok(LlmStreamEvent::TextDelta { .. })))
        .count();
    assert_eq!(words, 10);
}

/// The read that fails is the one that waited the bound, no more.
#[tokio::test(start_paused = true)]
async fn a_silent_stream_fails_at_exactly_the_bound() {
    let bytes = stream::iter([Ok::<_, std::io::Error>(frame("Hel"))]).chain(stream::pending());

    let started = tokio::time::Instant::now();
    let read = sse_events(bytes, IDLE).collect::<Vec<_>>();
    let events = tokio::time::timeout(PATIENCE, read).await.expect("it ends");

    assert_eq!(started.elapsed(), IDLE);
    let Some(Err(reason)) = events.last() else {
        panic!("the stream fails: {events:?}");
    };
    let named = format!("sent nothing for {IDLE:?}");
    assert!(reason.to_string().contains(&named), "{reason}");
}

/// `[DONE]` ends the stream though the server keeps the body open after it.
#[tokio::test(start_paused = true)]
async fn the_stream_ends_at_done_though_the_body_stays_open() {
    let frames = [frame("Hel"), b"data: [DONE]\n\n".to_vec()].map(Ok::<_, std::io::Error>);
    let bytes = stream::iter(frames).chain(stream::pending());

    let read = sse_events(bytes, IDLE).collect::<Vec<_>>();
    let events = tokio::time::timeout(PATIENCE, read).await.expect("it ends");

    assert!(events.iter().all(Result::is_ok), "{events:?}");
    let words = events
        .iter()
        .filter(|e| matches!(e, Ok(LlmStreamEvent::TextDelta { .. })))
        .count();
    assert_eq!(words, 1, "{events:?}");
}

/// A run gives up on a silent llama-server exactly when a proxy request does:
/// the bound is the proxy's, not a second number that could drift from it.
#[test]
fn a_runs_idle_bound_is_the_proxys() {
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:1", None);
    assert_eq!(
        adapter.stream_idle_timeout,
        gglib_proxy::STREAM_IDLE_TIMEOUT
    );
}
