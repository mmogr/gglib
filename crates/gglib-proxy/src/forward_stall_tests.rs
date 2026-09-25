//! A reply that goes silent mid-answer ends, within one idle bound, in a
//! notice, `upstream_timeout` and one `[DONE]`, with any tool call held back
//! from the client sent ahead of them; and the three things that look like
//! silence from the drain's side and are not.
//!
//! The bound times reads of the upstream, so three things that look like
//! silence from elsewhere are not stalls: an upstream that is slow but still
//! talking, a client slower to read than the bound, and a tool call whose
//! markup the normalizer holds back from the client until it is whole.

use std::time::Duration;

use serde_json::json;

use super::forward_stall_fixtures::{
    IDLE, Reader, Then, at_once, done, finish, frame, run_turn, run_turn_holding_back_tool_calls,
    text, upstream,
};
use super::*;

/// An upstream that answers "Hello" and then never says another word.
fn silent_after_hello()
-> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    upstream(at_once(vec![text("Hel"), text("lo")]), Then::Silence)
}

#[tokio::test]
async fn a_reply_that_goes_silent_mid_answer_ends_in_a_notice_upstream_timeout_and_one_done() {
    let turn = run_turn(silent_after_hello(), None, Reader::default()).await;

    let stall = turn
        .outcome
        .upstream_stalled
        .expect("the silence is a stall");
    assert_eq!(stall.after, IDLE);
    assert!(stall.after_first_token, "\"Hello\" came before the silence");
    assert!(turn.outcome.upstream_errored, "a stall is a death upstream");
    assert_eq!(
        turn.outcome.health_verdict(),
        StreamVerdict::Stalled {
            after_first_token: true
        }
    );

    // The answer so far, then the notice as more of the same turn's text.
    let shown = turn.text();
    let (answer, notice) = shown
        .split_once("\n\n")
        .expect("the notice follows the answer");
    assert_eq!(answer, "Hello");
    assert!(
        notice.contains("went silent") && notice.contains("this model is being recycled"),
        "{notice}"
    );
    let frames = turn.frames();
    let ids: Vec<_> = frames.iter().filter_map(|f| f.get("id")).collect();
    assert!(
        ids.windows(2).all(|pair| pair[0] == pair[1]),
        "the notice is sent under the turn's own id: {ids:?}"
    );

    // Then the error a client acts on, then exactly one [DONE], last.
    assert_eq!(turn.error_codes(), ["upstream_timeout"]);
    assert_eq!(turn.dones(), 1);
    assert_eq!(turn.payloads().last(), Some(&"[DONE]"));

    // Within one bound of the silence, and not before it.
    assert!(turn.took >= IDLE, "ended after {:?}", turn.took);
    assert!(turn.took < IDLE * 10, "ended after {:?}", turn.took);
}

#[tokio::test]
async fn a_held_back_tool_call_cut_off_by_a_stall_goes_out_before_the_notice_and_the_error() {
    // The call's first frame arrives and is held back until the call is
    // whole; then the upstream goes silent partway through its arguments.
    let call = json!({"tool_calls": [{
        "index": 0,
        "id": "call_1",
        "type": "function",
        "function": {"name": "read_file", "arguments": "{\"path\": \"src/"},
    }]});
    let turn = run_turn_holding_back_tool_calls(upstream(
        at_once(vec![frame(&call, None)]),
        Then::Silence,
    ))
    .await;

    assert!(turn.outcome.upstream_stalled.is_some(), "{}", turn.wire);
    let frames = turn.frames();
    let position = |found: &dyn Fn(&serde_json::Value) -> bool| {
        let at: Vec<_> = frames
            .iter()
            .enumerate()
            .filter_map(|(i, f)| found(f).then_some(i))
            .collect();
        assert_eq!(at.len(), 1, "exactly one such frame: {}", turn.wire);
        at[0]
    };
    let tool_call = position(&|f| f.pointer("/choices/0/delta/tool_calls").is_some());
    let notice = position(&|f| {
        f.pointer("/choices/0/delta/content")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|c| c.contains("went silent"))
    });
    let error = position(&|f| f.pointer("/error/code").is_some());

    // The call the model had begun goes out first: a client may read the
    // error frame as the end of the turn.
    assert!(tool_call < notice && notice < error, "{}", turn.wire);
    assert_eq!(turn.error_codes(), ["upstream_timeout"]);
    assert_eq!(turn.dones(), 1);
    assert_eq!(turn.payloads().last(), Some(&"[DONE]"));
}

#[tokio::test]
async fn an_upstream_that_is_slow_but_still_talking_is_never_cut() {
    let words = ["one ", "two ", "three ", "four ", "five ", "six"];
    let mut chunks: Vec<_> = words.iter().map(|w| (IDLE / 2, text(w))).collect();
    chunks.push((IDLE / 2, finish()));
    chunks.push((Duration::ZERO, done()));

    let turn = run_turn(upstream(chunks, Then::Close), None, Reader::default()).await;

    assert!(
        turn.took > IDLE * 3,
        "the reply outlasted the bound several times over: {:?}",
        turn.took
    );
    assert_eq!(turn.outcome.upstream_stalled, None);
    assert_eq!(turn.outcome.health_verdict(), StreamVerdict::Healthy);
    assert_eq!(turn.text(), words.concat());
    assert!(turn.error_codes().is_empty(), "{}", turn.wire);
    assert_eq!(turn.dones(), 1);
}

#[tokio::test]
async fn a_client_slower_than_the_bound_is_not_mistaken_for_a_silent_upstream() {
    // Each chunk comes half a bound after the drain asks for it, and the
    // drain asks only once the client has taken the previous frame.
    let chunks = [text("Hel"), text("lo"), finish(), done()]
        .into_iter()
        .map(|chunk| (IDLE / 2, chunk))
        .collect();
    let slow = Reader {
        delay: IDLE + IDLE / 3,
        leaves_after: None,
    };

    let turn = run_turn(upstream(chunks, Then::Close), None, slow).await;

    // The drain waited on the client longer than the bound, more than once,
    // and each chunk came more than a bound after the one before: the
    // upstream's reads were not being timed while the drain waited.
    assert!(turn.took > IDLE * 2, "took {:?}", turn.took);
    assert_eq!(turn.outcome.upstream_stalled, None);
    assert!(!turn.outcome.client_aborted);
    assert_eq!(turn.outcome.health_verdict(), StreamVerdict::Healthy);
    assert_eq!(turn.text(), "Hello");
    assert_eq!(turn.dones(), 1);
}

#[tokio::test]
async fn a_tool_call_still_assembling_is_not_cut_while_its_markup_keeps_arriving() {
    // A Qwen model writes its call as text, which the normalizer holds back
    // until it is whole: the client gets no frame for the whole of this.
    let markup = [
        "<tool_call>\n",
        "<function=read_file>\n",
        "<parameter=path>\n",
        "src/lib.rs\n",
        "</parameter>\n",
        "</function>\n",
        "</tool_call>",
    ];
    let mut chunks: Vec<_> = markup
        .iter()
        .map(|m| (IDLE / 2, frame(&json!({ "content": m }), None)))
        .collect();
    chunks.push((Duration::ZERO, finish()));
    chunks.push((Duration::ZERO, done()));

    let turn = run_turn(
        upstream(chunks, Then::Close),
        Some(DialectSpec::qwen_xml()),
        Reader::default(),
    )
    .await;

    assert!(turn.took > IDLE * 3, "took {:?}", turn.took);
    assert_eq!(turn.outcome.upstream_stalled, None);
    assert!(turn.error_codes().is_empty(), "{}", turn.wire);
    let called: Vec<_> = turn
        .frames()
        .iter()
        .filter_map(|f| {
            f.pointer("/choices/0/delta/tool_calls/0/function/name")?
                .as_str()
        })
        .map(str::to_owned)
        .collect();
    assert_eq!(called, ["read_file"], "{}", turn.wire);
    assert_eq!(turn.dones(), 1);
}
