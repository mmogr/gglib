//! A stream that goes silent mid-answer, and the request behind it, through a
//! real `serve`.
//!
//! The upstream wedges after one word (see `fixtures::stall`). The first
//! request must end within one idle bound with `upstream_timeout`, free the
//! model, and ask for a recycle. What happens to a second request for the same
//! model depends on where it was waiting:
//!
//! * in admission, which is where `SERVER_PARALLEL = 1` holds it: the proxy
//!   carries out the recycle before forwarding it, and the fresh model answers;
//! * already inside the wedged upstream: it gives up at its first-byte
//!   deadline without being sent again, and the next request gets the recycle.

mod fixtures;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use fixtures::common::parse_sse_frames;
use fixtures::stall::{BOUNDS, MODEL, StallRuntime, spawn_proxy, spawn_upstream};

/// No step of these tests should take anywhere near this long; a mutation
/// that removes a bound or wedges admission fails here instead of hanging.
const PATIENCE: Duration = Duration::from_secs(10);

/// Send one streaming chat request and return its response once headers
/// arrive.
async fn ask(base: &str, prompt: &str) -> reqwest::Response {
    let body = json!({
        "model": MODEL,
        "messages": [{"role": "user", "content": prompt}],
        "stream": true,
    });
    let send = reqwest::Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&body)
        .send();
    let resp = tokio::time::timeout(PATIENCE, send)
        .await
        .expect("the proxy answers within PATIENCE")
        .expect("the proxy answers");
    assert_eq!(resp.status(), 200);
    resp
}

/// The streamed body as text, read until the stream ends, within `PATIENCE`.
async fn read_body(resp: reqwest::Response) -> String {
    tokio::time::timeout(PATIENCE, resp.text())
        .await
        .expect("the stream ends")
        .expect("the stream reads")
}

/// Read until `word` has arrived, and hand the rest of the stream back.
async fn read_until(
    resp: reqwest::Response,
    word: &str,
) -> impl futures_util::Stream<Item = reqwest::Result<bytes::Bytes>> {
    let mut stream = resp.bytes_stream();
    let mut seen = String::new();
    while !seen.contains(word) {
        let chunk = tokio::time::timeout(PATIENCE, stream.next())
            .await
            .expect("the first word arrives")
            .expect("the stream is open")
            .expect("the stream reads");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
    stream
}

/// The joined visible text of a body, and its error codes.
fn text_and_errors(body: &str) -> (String, Vec<String>) {
    let (frames, saw_done) = parse_sse_frames(body);
    assert!(saw_done, "the stream ends in [DONE]: {body}");
    let text = frames
        .iter()
        .filter_map(|f| f.pointer("/choices/0/delta/content")?.as_str())
        .collect();
    let codes = frames
        .iter()
        .filter_map(|f| f.pointer("/error/code")?.as_str().map(str::to_owned))
        .collect();
    (text, codes)
}

/// The dashboard snapshot, once no request is in flight: every task has
/// finished, its KV-cache save included.
async fn status_when_idle(base: &str) -> Value {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let status: Value = reqwest::get(format!("{base}/v1/proxy/status"))
            .await
            .expect("status answers")
            .json()
            .await
            .expect("status is JSON");
        if status["active_connections"]
            .as_array()
            .is_some_and(Vec::is_empty)
        {
            return status;
        }
        assert!(Instant::now() < deadline, "still busy: {status}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_request_queued_in_admission_behind_a_stalled_stream_is_served_by_the_recycled_model() {
    let cancel = CancellationToken::new();
    let slots = tempfile::tempdir().expect("a temp dir");
    let (port, upstream) = spawn_upstream(slots.path().to_owned(), cancel.clone()).await;
    let runtime = Arc::new(StallRuntime::new(port, Arc::clone(&upstream), true));
    let base = spawn_proxy(
        Arc::clone(&runtime),
        Some(slots.path().to_owned()),
        cancel.clone(),
    )
    .await;

    let first = ask(&base, "first").await;
    let rest_of_first = read_until(first, "Hel").await;
    let silent_since = Instant::now();
    // Sent while the first is still in flight, so it waits in admission.
    let second = tokio::spawn({
        let base = base.clone();
        async move { read_body(ask(&base, "second").await).await }
    });

    let rest: Vec<_> = tokio::time::timeout(PATIENCE, rest_of_first.collect())
        .await
        .expect("the stalled stream ends");
    let ended_after = silent_since.elapsed();
    let rest: String = rest
        .into_iter()
        .map(|chunk| String::from_utf8_lossy(&chunk.expect("reads")).into_owned())
        .collect();
    let (notice, codes) = text_and_errors(&rest);
    assert!(notice.contains("this model is being recycled"), "{rest}");
    assert_eq!(codes, ["upstream_timeout"]);
    assert!(
        ended_after < BOUNDS.idle * 10,
        "ended after {ended_after:?}"
    );

    let second = tokio::time::timeout(PATIENCE * 2, second)
        .await
        .expect("the second request ends");
    let (answer, codes) = text_and_errors(&second.expect("the second request ran"));
    assert_eq!(answer, "fresh", "the recycled model answered it");
    assert!(codes.is_empty(), "{codes:?}");

    // One recycle, carried out when only the stalled request had reached the
    // wedged server: the second was never sent to it. The recycle ran while
    // the second held the one place in admission, so no other request could
    // have been admitted onto the server being stopped.
    assert_eq!(*runtime.recycles.lock().unwrap(), [1]);
    assert_eq!(*runtime.admission_open_at_recycle.lock().unwrap(), [false]);
    assert_eq!(upstream.posts_while_wedged.load(Ordering::SeqCst), 1);
    assert_eq!(upstream.posts_after_recycle.load(Ordering::SeqCst), 1);

    let status = status_when_idle(&base).await;
    assert_eq!(status["upstream_health"]["total_stream_stalls"], 1);
    assert_eq!(status["upstream_health"]["total_recycles"], 1);
    // The second turn's KV cache was saved; the stalled one's was not asked for.
    assert_eq!(upstream.saves.load(Ordering::SeqCst), 1);
    cancel.cancel();
}

#[tokio::test]
async fn a_request_already_waiting_on_the_wedged_upstream_gives_up_once_and_the_next_is_recycled() {
    let cancel = CancellationToken::new();
    // The cache is off, so nothing is ever saved into this directory.
    let (port, upstream) = spawn_upstream(std::env::temp_dir(), cancel.clone()).await;
    let runtime = Arc::new(StallRuntime::new(port, Arc::clone(&upstream), false));
    let base = spawn_proxy(Arc::clone(&runtime), None, cancel.clone()).await;

    let first = ask(&base, "first").await;
    let rest_of_first = read_until(first, "Hel").await;
    // Admitted at once, so it goes straight to the upstream, which has no
    // slot for it: it waits on its first-byte deadline, extended while the
    // first request is generating.
    let second = ask(&base, "second").await;
    let started = Instant::now();
    let first_ends = tokio::time::timeout(PATIENCE, rest_of_first.count());
    let (first_ended, second_body) = tokio::join!(first_ends, read_body(second));
    first_ended.expect("the stalled stream ends");

    // Once the first stalled and asked for a recycle, the second's next
    // deadline ended it: no second send into the wedged server.
    let (_, codes) = text_and_errors(&second_body);
    assert_eq!(codes, ["upstream_timeout"], "{second_body}");
    assert!(second_body.contains("did not respond"), "{second_body}");
    assert_eq!(upstream.posts_while_wedged.load(Ordering::SeqCst), 2);
    let waited = started.elapsed();
    assert!(waited < BOUNDS.idle * 10, "gave up after {waited:?}");
    assert!(runtime.recycles.lock().unwrap().is_empty());

    // Nobody was left in flight, so the next request carries out the recycle.
    let (answer, _) = text_and_errors(&read_body(ask(&base, "third").await).await);
    assert_eq!(answer, "fresh");
    assert_eq!(*runtime.recycles.lock().unwrap(), [2]);
    cancel.cancel();
}
