//! The shim's replies as gglib reads them: gglib-core's own SSE decoder over
//! the stream, the answer not streamed, the ways a reply ends, and a client
//! that hangs up.

use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use gglib_core::LlmStreamEvent;
use serde_json::json;

use super::test_support::{ask, decoded, frames, json_of, running, running_in, streamed};
use crate::decoder::{FinishReason, Sampling, Script};

#[tokio::test]
async fn gglib_cores_sse_decoder_reads_progress_text_finish_and_usage() {
    let mut script = Script::new(["Hello", ", ", "world"]);
    script.batch = 8;
    let shim = running(script).await;
    let (status, body) = shim
        .post("/v1/chat/completions", &streamed("Say hello"))
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.ends_with("data: [DONE]\n\n"), "{body}");

    let prompt = shim.record().prompts[0].chars().count();
    let events = decoded(&body);
    let progress: Vec<(u32, u32, u32)> = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::PromptProgress {
                processed,
                total,
                cached,
                ..
            } => Some((*processed, *total, *cached)),
            _ => None,
        })
        .collect();
    assert!(progress.len() > 2, "{progress:?}");
    assert_eq!(
        progress.first(),
        Some(&(0, u32::try_from(prompt).unwrap(), 0))
    );
    assert!(progress.windows(2).all(|w| w[0].0 < w[1].0), "{progress:?}");
    assert_eq!(
        progress.last().map(|p| p.0),
        Some(u32::try_from(prompt).unwrap())
    );

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::TextDelta { content } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "Hello, world");
    assert!(events.iter().any(
        |e| matches!(e, LlmStreamEvent::Done { finish_reason } if finish_reason.as_deref() == Some("stop"))
    ));
    let usage = events.iter().find_map(|e| match e {
        LlmStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
            ..
        } => Some((
            *prompt_tokens,
            *completion_tokens,
            *total_tokens,
            *cached_tokens,
        )),
        _ => None,
    });
    let prompt = u32::try_from(prompt).unwrap();
    // Three pieces and the end-of-generation token after them.
    assert_eq!(usage, Some((prompt, 4, prompt + 4, Some(0))));
}

#[tokio::test]
async fn the_same_chat_again_reads_its_prompt_from_the_cache() {
    let shim = running(Script::new(["ok"])).await;
    let body = streamed("Again and again");
    shim.post("/v1/chat/completions", &body).await;
    let (_, again) = shim.post("/v1/chat/completions", &body).await;
    let usage = frames(&again)
        .into_iter()
        .find_map(|f| f.get("usage").cloned())
        .expect("a usage chunk");
    let prompt = usage["prompt_tokens"].as_u64().unwrap();
    assert_eq!(
        usage["prompt_tokens_details"]["cached_tokens"],
        json!(prompt - 1)
    );
}

#[tokio::test]
async fn a_reply_not_streamed_is_one_completion_with_usage_and_timings() {
    let shim = running(Script::new(["Hello", ", ", "world"])).await;
    let (status, body) = shim.post("/v1/chat/completions", &ask("Say hello")).await;
    assert_eq!(status, 200, "{body}");
    let answer = json_of(&body);
    assert_eq!(answer["object"], json!("chat.completion"));
    assert_eq!(
        answer["choices"][0]["message"]["content"],
        json!("Hello, world")
    );
    assert_eq!(answer["choices"][0]["message"]["role"], json!("assistant"));
    assert_eq!(answer["choices"][0]["finish_reason"], json!("stop"));
    assert_eq!(answer["usage"]["completion_tokens"], json!(4));
    assert_eq!(answer["timings"]["predicted_n"], json!(4));
}

#[tokio::test]
async fn max_tokens_ends_the_reply_with_length() {
    let shim = running(Script::new(["a", "b", "c", "d"])).await;
    let mut body = ask("Count");
    body["max_tokens"] = json!(2);
    let answer = json_of(&shim.post("/v1/chat/completions", &body).await.1);
    assert_eq!(answer["choices"][0]["message"]["content"], json!("ab"));
    assert_eq!(answer["choices"][0]["finish_reason"], json!("length"));
    assert_eq!(answer["usage"]["completion_tokens"], json!(2));
}

#[tokio::test]
async fn a_stop_string_cuts_the_reply_and_ends_the_generation() {
    // A step long enough that the reply finds the stop before the next one,
    // on a slow runner too.
    let mut script = Script::new(["one ", "tw", "o three", " four", " five"]);
    script.step_delay = Duration::from_millis(200);
    let shim = running(script).await;
    let mut body = streamed("Count");
    body["stop"] = json!(["two"]);
    let (_, reply) = shim.post("/v1/chat/completions", &body).await;
    let events = decoded(&reply);
    let text: String = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::TextDelta { content } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "one ");
    assert!(events.iter().any(
        |e| matches!(e, LlmStreamEvent::Done { finish_reason } if finish_reason.as_deref() == Some("stop"))
    ));
    let record = shim.record();
    assert_eq!(record.finished[0].reason, FinishReason::Aborted);
    assert!(record.pieces < 5, "{} pieces were sampled", record.pieces);
}

#[tokio::test]
async fn the_request_s_sampling_reaches_the_decoder() {
    let shim = running(Script::new(["ok"])).await;
    let mut body = ask("Hi");
    body["temperature"] = json!(0.2);
    body["top_k"] = json!(7);
    body["seed"] = json!(42);
    shim.post("/v1/chat/completions", &body).await;
    assert_eq!(shim.record().samplings, vec![Sampling::from_request(&body)]);
}

#[tokio::test]
async fn a_client_that_hangs_up_mid_stream_aborts_the_decode_within_one_step() {
    // Twenty seconds of reply, in a context it fits.
    let mut script = Script::new(vec!["tick "; 10_000]);
    script.step_delay = Duration::from_millis(2);
    let shim = running_in(script, 1 << 20).await;
    let response = shim.send("/v1/chat/completions", &streamed("Go on")).await;
    let mut bytes = response.bytes_stream();
    let mut seen = String::new();
    while !seen.contains("tick") {
        let chunk = bytes
            .next()
            .await
            .expect("more of the stream")
            .expect("bytes");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
    drop(bytes);

    let deadline = Instant::now() + Duration::from_secs(10);
    let record = loop {
        let record = shim.record();
        if !record.finished.is_empty() {
            break record;
        }
        assert!(
            Instant::now() < deadline,
            "the decode ran on after the client left"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(record.finished[0].reason, FinishReason::Aborted);
    assert!(
        record.pieces < 10_000,
        "{} pieces were sampled",
        record.pieces
    );
    assert!(
        record.pieces_after_abort <= 1,
        "{} pieces were sampled after the abort",
        record.pieces_after_abort
    );
}
