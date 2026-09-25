//! Fixtures for the stall tests: an upstream that talks, pauses and goes
//! silent on a schedule, a client that reads at its own pace, and the drain
//! run between them.
//!
//! A module of its own so both stall test files can use them. Real time
//! throughout, with a short idle bound: a paused clock does not promise the
//! woken drain is polled before the test looks (see `dashboard_tests.rs`).

use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream, Stream};
use serde_json::{Value, json};

use super::*;
use crate::upstream_read::upstream_events;

/// The idle bound these tests run the drain under. Long enough that a busy
/// runner does not cut a stream that sends every half of it, short enough
/// that a silent one ends quickly.
pub(super) const IDLE: Duration = Duration::from_millis(300);

/// One SSE frame whose delta is `delta`, with `finish_reason`.
pub(super) fn frame(delta: &Value, finish_reason: Option<&str>) -> Bytes {
    let frame = json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]});
    Bytes::from(format!("data: {frame}\n\n"))
}

/// One frame of answer text.
pub(super) fn text(content: &str) -> Bytes {
    frame(&json!({ "content": content }), None)
}

/// The frame that ends the turn: an empty delta with `finish_reason: stop`.
pub(super) fn finish() -> Bytes {
    frame(&json!({}), Some("stop"))
}

/// A `prompt_progress` frame, which llama-server sends during prefill.
pub(super) fn progress() -> Bytes {
    Bytes::from_static(
        b"data: {\"prompt_progress\":{\"cache\":0,\"processed\":10,\"total\":57,\"time_ms\":5}}\n\n",
    )
}

/// The sentinel llama-server ends its body with.
pub(super) fn done() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

/// How a scripted upstream ends once its chunks are sent.
pub(super) enum Then {
    /// It closes the body.
    Close,
    /// It keeps the connection open and never sends another byte.
    Silence,
}

/// An upstream that sends each chunk once its delay has passed, then does
/// what `then` says.
pub(super) fn upstream(
    chunks: Vec<(Duration, Bytes)>,
    then: Then,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    let sent = stream::iter(chunks).then(|(delay, chunk)| async move {
        tokio::time::sleep(delay).await;
        Ok::<_, std::io::Error>(chunk)
    });
    let rest: BoxStream<'static, _> = match then {
        Then::Close => stream::empty().boxed(),
        Then::Silence => stream::pending().boxed(),
    };
    sent.chain(rest)
}

/// Every chunk at once.
pub(super) fn at_once(chunks: Vec<Bytes>) -> Vec<(Duration, Bytes)> {
    chunks.into_iter().map(|c| (Duration::ZERO, c)).collect()
}

/// How the client reads the turn.
#[derive(Clone, Copy, Default)]
pub(super) struct Reader {
    /// How long it waits before taking each frame.
    pub(super) delay: Duration,
    /// It hangs up after taking this many frames.
    pub(super) leaves_after: Option<usize>,
}

/// One turn as the client saw it, and what the drain made of it.
pub(super) struct Turn {
    /// Every byte the client took, in order.
    pub(super) wire: String,
    /// What the drain reported.
    pub(super) outcome: StreamOutcome,
    /// How long the drain ran.
    pub(super) took: Duration,
}

impl Turn {
    /// The `data:` payloads, `[DONE]` included, in order.
    pub(super) fn payloads(&self) -> Vec<&str> {
        self.wire
            .split("\n\n")
            .filter_map(|f| f.trim_start().strip_prefix("data: "))
            .collect()
    }

    /// How many `[DONE]` sentinels the client got.
    pub(super) fn dones(&self) -> usize {
        self.payloads().iter().filter(|p| **p == "[DONE]").count()
    }

    /// The JSON frames, `[DONE]` left out.
    pub(super) fn frames(&self) -> Vec<Value> {
        self.payloads()
            .into_iter()
            .filter(|p| *p != "[DONE]")
            .map(|p| serde_json::from_str(p).expect("every data frame is JSON"))
            .collect()
    }

    /// The visible text, joined across frames.
    pub(super) fn text(&self) -> String {
        self.frames()
            .iter()
            .filter_map(|f| {
                f.pointer("/choices/0/delta/content")?
                    .as_str()
                    .map(str::to_owned)
            })
            .collect()
    }

    /// The `error.code` of every error frame.
    pub(super) fn error_codes(&self) -> Vec<String> {
        self.frames()
            .iter()
            .filter_map(|f| f.pointer("/error/code")?.as_str().map(str::to_owned))
            .collect()
    }
}

/// Run the drain over `bytes` at [`IDLE`], with a one-frame channel so that a
/// client slower than the upstream holds the drain at `tx.send`, as a slow
/// socket does.
pub(super) async fn run_turn(
    bytes: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    dialect: Option<DialectSpec>,
    pace: Reader,
) -> Turn {
    run_turn_with(bytes, dialect, pace, None).await
}

/// [`run_turn`] with the tool-call hold-back engaged. The repair context's
/// endpoint is never contacted: only a turn's `Done` can start a re-issue.
pub(super) async fn run_turn_holding_back_tool_calls(
    bytes: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> Turn {
    let repair = RepairContext {
        req_builder: crate::loopback::client().post("http://127.0.0.1:9/"),
        request_body: Bytes::new(),
        turn: RepairTurn::ON,
    };
    run_turn_with(bytes, None, Reader::default(), Some(repair)).await
}

async fn run_turn_with(
    bytes: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    dialect: Option<DialectSpec>,
    pace: Reader,
    repair: Option<RepairContext>,
) -> Turn {
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(1);

    let client = tokio::spawn(async move {
        let mut wire = String::new();
        let mut taken = 0;
        while pace.leaves_after != Some(taken) {
            tokio::time::sleep(pace.delay).await;
            let Some(Ok(chunk)) = rx.recv().await else {
                break;
            };
            wire.push_str(&String::from_utf8_lossy(&chunk));
            taken += 1;
        }
        wire
    });

    let started = Instant::now();
    let events = upstream_events(bytes, IDLE);
    let drain = drain_events(
        events,
        "m".to_owned(),
        dialect,
        tx,
        &connection,
        repair,
        false,
    );
    // A drain that never ends fails the test instead of hanging it.
    let outcome = tokio::time::timeout(IDLE * 30, drain)
        .await
        .expect("the drain ends");
    let took = started.elapsed();
    let wire = client.await.expect("the client task does not panic");
    Turn {
        wire,
        outcome,
        took,
    }
}
