//! A streamed reply goes to the client as llama-server wrote it: each frame,
//! comments included, with only the edits of [`crate::frame_edit`], and one
//! `[DONE]` of the drain's own.

use serde_json::{Value, json};

use super::forward_stall_fixtures::{
    Reader, Then, at_once, run_turn, run_turn_holding_back_tool_calls, upstream,
};
use super::*;
use crate::llama_frame_fixtures::{
    UPSTREAM_ID, done_bytes, llama_bytes, llama_frame, llama_frame_at, role_bytes, sse,
};

fn reasoning(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::ReasoningDelta {
        content: content.to_owned(),
    }
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn finish(reason: &str) -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some(reason.to_owned()),
    }
}

fn call(index: usize, id: Option<&str>, arguments: &str) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: id.map(str::to_owned),
        name: id.map(|_| "read_file".to_owned()),
        arguments: Some(arguments.to_owned()),
    }
}

fn usage() -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 5,
        completion_tokens: 9,
        total_tokens: 14,
        cached_tokens: Some(0),
    }
}

/// `frame` as the drain's client gets it: named as the served model, "m".
fn served(mut frame: Value) -> Value {
    frame["model"] = json!("m");
    frame
}

/// Every upstream frame comes out, in order, with only `model` changed; the
/// role frame and the usage frame's `timings` included, and one `[DONE]`.
#[tokio::test]
async fn a_reply_goes_out_frame_for_frame_with_only_its_model_changed() {
    let events = [reasoning("hm"), text("hi"), finish("stop"), usage()];
    let mut chunks = vec![role_bytes()];
    chunks.extend(events.iter().map(llama_bytes));
    chunks.push(done_bytes());
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    let role: Value = serde_json::from_slice(&role_bytes()[6..]).expect("JSON");
    let mut want = vec![served(role)];
    want.extend(events.iter().map(|e| served(llama_frame(e))));
    assert_eq!(turn.frames(), want);
    assert_eq!(turn.dones(), 1, "{}", turn.wire);
    assert!(turn.wire.ends_with("data: [DONE]\n\n"));
}

/// A frame no edit changes is sent as the bytes that came, key order and
/// all: here one that names no model.
#[tokio::test]
async fn a_frame_no_edit_changes_goes_out_as_its_bytes() {
    let odd = "{\"z\":1,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}";
    let chunks = vec![Bytes::from(format!("data: {odd}\n\n")), done_bytes()];
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    assert_eq!(turn.wire, format!("data: {odd}\n\ndata: [DONE]\n\n"));
}

/// The upstream's comments reach the client where they were sent.
#[tokio::test]
async fn comments_pass_through_in_place() {
    let chunks = vec![
        Bytes::from_static(b": ping\n\n"),
        llama_bytes(&text("a")),
        Bytes::from_static(b":\n\n"),
        llama_bytes(&finish("stop")),
        done_bytes(),
    ];
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    let blocks: Vec<&str> = turn.wire.split_inclusive("\n\n").collect();
    assert_eq!(blocks.len(), 5, "{}", turn.wire);
    assert_eq!(blocks[0], ": ping\n\n");
    assert!(blocks[1].starts_with("data: "));
    assert_eq!(blocks[2], ":\n\n");
}

/// A reply that stops with no finish frame ends with what it sent: the drain
/// writes no finish of its own, only the `[DONE]`.
#[tokio::test]
async fn a_reply_cut_off_gets_no_finish_from_gglib() {
    let chunks = vec![llama_bytes(&text("hal"))];
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    assert_eq!(turn.frames(), vec![served(llama_frame(&text("hal")))]);
    assert_eq!(turn.dones(), 1);
    assert_eq!(turn.outcome.finish_reason, None);
}

/// A mid-stream error keeps its whole object, `n_ctx` and all, with the
/// numeric code llama-server writes given as `upstream_error`.
#[tokio::test]
async fn a_mid_stream_error_keeps_its_object_with_a_string_code() {
    let error = json!({"error": {
        "code": 400, "message": "context full", "type": "exceed_context_size_error",
        "n_ctx": 4096, "n_prompt_tokens": 5000
    }});
    let chunks = vec![llama_bytes(&text("a")), sse(&error)];
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    let mut want = error;
    want["error"]["code"] = json!("upstream_error");
    assert_eq!(turn.frames().last(), Some(&want));
    assert_eq!(turn.error_codes(), ["upstream_error"]);
    assert!(turn.outcome.upstream_errored);
}

/// gglib's own frames, here the reasoning-only notice, carry the upstream's
/// `id` and the `created` of its last frame, which moves during a reply.
#[tokio::test]
async fn gglibs_own_frames_take_the_upstreams_id_and_last_created() {
    let chunks = vec![
        sse(&llama_frame_at(&reasoning("so"), 100)),
        sse(&llama_frame_at(&finish("stop"), 107)),
        done_bytes(),
    ];
    let turn = run_turn(upstream(at_once(chunks), Then::Close), Reader::default()).await;

    let frames = turn.frames();
    let notice = frames.last().expect("the notice");
    assert!(
        notice["choices"][0]["delta"]["content"]
            .as_str()
            .is_some_and(|c| c.contains("reasoning-only")),
        "{notice}"
    );
    assert_eq!(notice["id"], json!(UPSTREAM_ID));
    assert_eq!(notice["created"], json!(107));
    assert_eq!(notice["model"], json!("m"));
}

/// Under the hold-back, a call's frames reach the client as they came, ahead
/// of the finish frame, each once.
#[tokio::test]
async fn held_call_frames_go_out_unchanged_ahead_of_the_finish() {
    let events = [
        call(0, Some("c1"), "{\"path\":"),
        call(0, None, "\"a\"}"),
        finish("tool_calls"),
        usage(),
    ];
    let mut chunks: Vec<Bytes> = events.iter().map(llama_bytes).collect();
    chunks.push(done_bytes());
    let turn = run_turn_holding_back_tool_calls(upstream(at_once(chunks), Then::Close)).await;

    let want: Vec<Value> = events.iter().map(|e| served(llama_frame(e))).collect();
    assert_eq!(turn.frames(), want);
    assert_eq!(turn.outcome.tool_calls, 1);
}

/// A finish frame that also carries a call's last fragment is held with the
/// call, and goes out once, in its place.
#[tokio::test]
async fn a_finish_that_carries_a_call_goes_out_once_after_it() {
    let opens = llama_frame(&call(0, Some("c1"), "{\"path\":"));
    let mut closes = llama_frame(&call(0, None, "\"a\"}"));
    closes["choices"][0]["finish_reason"] = json!("tool_calls");
    let chunks = vec![sse(&opens), sse(&closes), done_bytes()];
    let turn = run_turn_holding_back_tool_calls(upstream(at_once(chunks), Then::Close)).await;

    assert_eq!(turn.frames(), vec![served(opens), served(closes)]);
    assert_eq!(turn.outcome.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(turn.dones(), 1);
}
