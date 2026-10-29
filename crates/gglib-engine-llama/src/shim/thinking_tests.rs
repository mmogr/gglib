//! A thinking model's reply over the wire: its reasoning sent as the pinned
//! llama-server sends it, then its answer, and neither carrying a tag.

use gglib_core::LlmStreamEvent;
use serde_json::json;

use super::test_support::{ask, decoded, json_of, running, streamed};
use crate::decoder::Script;

/// Qwen3-0.6B's template at the pin: thinking on, the model opens `<think>`.
const QWEN3: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen-Qwen3-0.6B.jinja");
/// Qwen3.5-4B's template at the pin: thinking on, its generation prompt
/// opens `<think>`.
const QWEN35: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen3.5-4B.jinja");

/// A model on `template` that thinks "Rome it is." and answers "Rome.".
fn thinker(template: &str) -> Script {
    let opens = (template == QWEN3).then_some("<think>\n");
    let pieces = opens
        .into_iter()
        .chain(["Rome", " it is.", "\n</th", "ink>\n\n", "Rome."]);
    let mut script = Script::new(pieces);
    script.chat_template = template.to_owned();
    script
}

#[tokio::test]
async fn gglib_cores_decoder_reads_the_reasoning_then_the_answer_without_tags() {
    for template in [QWEN35, QWEN3] {
        let shim = running(thinker(template)).await;
        let (status, body) = shim
            .post("/v1/chat/completions", &streamed("And of Italy?"))
            .await;
        assert_eq!(status, 200, "{body}");
        let read: Vec<(bool, String)> = decoded(&body)
            .into_iter()
            .filter_map(|e| match e {
                LlmStreamEvent::ReasoningDelta { content } => Some((true, content)),
                LlmStreamEvent::TextDelta { content } => Some((false, content)),
                _ => None,
            })
            .collect();
        let of = |reasoning: bool| -> String {
            read.iter()
                .filter(|(r, _)| *r == reasoning)
                .map(|(_, text)| text.as_str())
                .collect()
        };
        assert_eq!(of(true), "Rome it is.\n", "{read:?}");
        assert_eq!(of(false), "Rome.", "{read:?}");
        let first_answer = read.iter().position(|(r, _)| !r);
        let last_reasoning = read.iter().rposition(|(r, _)| *r);
        assert!(last_reasoning < first_answer, "{read:?}");
    }
}

#[tokio::test]
async fn a_reply_not_streamed_carries_its_reasoning_beside_its_answer() {
    let shim = running(thinker(QWEN35)).await;
    let answer = json_of(&shim.post("/v1/chat/completions", &ask("Hi")).await.1);
    let message = &answer["choices"][0]["message"];
    assert_eq!(message["content"], json!("Rome."));
    assert_eq!(message["reasoning_content"], json!("Rome it is.\n"));

    let plain = running(Script::new(["Hello"])).await;
    let answer = json_of(&plain.post("/v1/chat/completions", &ask("Hi")).await.1);
    let message = &answer["choices"][0]["message"];
    assert_eq!(message["content"], json!("Hello"));
    assert_eq!(message.get("reasoning_content"), None, "{answer}");
}

#[tokio::test]
async fn reasoning_format_none_sends_the_reply_as_it_came() {
    let shim = running(thinker(QWEN35)).await;
    let mut body = ask("Hi");
    body["reasoning_format"] = json!("none");
    let answer = json_of(&shim.post("/v1/chat/completions", &body).await.1);
    let message = &answer["choices"][0]["message"];
    assert_eq!(message["content"], json!("Rome it is.\n</think>\n\nRome."));
    assert_eq!(message.get("reasoning_content"), None, "{answer}");
}

#[tokio::test]
async fn a_follow_up_turn_reads_the_last_reply_from_the_cache() {
    let shim = running(Script::new(["Paris."])).await;
    let first = json!({"messages": [{"role": "user", "content": "France?"}]});
    let answer = json_of(&shim.post("/v1/chat/completions", &first).await.1);
    let read = answer["usage"]["prompt_tokens"].as_u64().unwrap();

    let next = json!({"messages": [
        {"role": "user", "content": "France?"},
        {"role": "assistant", "content": "Paris."},
        {"role": "user", "content": "Italy?"},
    ]});
    let answer = json_of(&shim.post("/v1/chat/completions", &next).await.1);
    // The first prompt and the reply sampled after it, a token a character.
    assert_eq!(
        answer["usage"]["prompt_tokens_details"]["cached_tokens"],
        json!(read + 6)
    );
}
