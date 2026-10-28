//! A reply split as the pinned llama-server splits it, piece by piece, and
//! where each pinned template's reply starts.

use gglib_gguf::{ChatTemplate, VocabTokens};
use serde_json::{Value, json};

use super::{Part, ReasoningSplit, Start, rendered};

const QWEN3: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen-Qwen3-0.6B.jinja");
const QWEN35: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen3.5-4B.jinja");
const GRANITE: &str =
    include_str!("../../../gglib-gguf/src/render/testdata/ibm-granite-granite-4.0.jinja");

/// The reasoning and the answer `pieces` split into from `start`, the
/// split's own parts kept to check how they were cut.
fn split(start: Start, pieces: &[&str]) -> (String, String, Vec<Part>) {
    let mut s = ReasoningSplit::new(start);
    let mut parts: Vec<Part> = pieces.iter().flat_map(|p| s.push(p)).collect();
    parts.extend(s.finish());
    let (mut reasoning, mut answer) = (String::new(), String::new());
    for part in &parts {
        match part {
            Part::Reasoning(text) => reasoning.push_str(text),
            Part::Answer(text) => answer.push_str(text),
        }
    }
    (reasoning, answer, parts)
}

#[test]
fn reasoning_the_prompt_opened_ends_at_its_tag_however_the_tag_is_cut() {
    // test-chat.cpp's session test at the pin: Qwen3.5 thinking, its
    // generation prompt ending `<think>\n`.
    let (reasoning, answer, parts) = split(
        Start::Reasoning,
        &["I'm\nthi", "nking\n</th", "ink>\n", "\nHello, "],
    );
    assert_eq!(reasoning, "I'm\nthinking\n");
    assert_eq!(answer, "Hello, ");
    assert!(
        parts
            .iter()
            .all(|p| !matches!(p, Part::Reasoning(t) | Part::Answer(t) if t.contains('<'))),
        "{parts:?}"
    );
}

#[test]
fn a_block_the_reply_opens_is_reasoning() {
    let (reasoning, answer, _) = split(
        Start::MayOpen,
        &["\n", "<th", "ink>\nI'm thinking", "\n</think>\n\n", "Hello"],
    );
    assert_eq!(reasoning, "I'm thinking\n");
    assert_eq!(answer, "Hello");
}

#[test]
fn a_block_after_the_first_word_is_the_answers() {
    let (reasoning, answer, _) = split(Start::MayOpen, &["\nSure. ", "<think>no</think>"]);
    assert_eq!(reasoning, "");
    assert_eq!(answer, "Sure. <think>no</think>");
}

#[test]
fn a_reply_after_a_closed_block_is_all_answer_tags_and_all() {
    let (reasoning, answer, _) = split(Start::Answer, &["<think>x", "</think>y"]);
    assert_eq!(reasoning, "");
    assert_eq!(answer, "<think>x</think>y");
}

#[test]
fn whitespace_alone_is_no_reasoning() {
    let (reasoning, answer, parts) = split(Start::Reasoning, &["\n\n", "</think>\n\nHi"]);
    assert_eq!(reasoning, "");
    assert_eq!(answer, "Hi");
    assert_eq!(parts, vec![Part::Answer("Hi".to_owned())]);
}

#[test]
fn a_reply_cut_off_mid_thought_is_all_reasoning_even_a_tag_begun() {
    let (reasoning, answer, _) = split(Start::Reasoning, &["half a thought </thi"]);
    assert_eq!(reasoning, "half a thought </thi");
    assert_eq!(answer, "");
    let (reasoning, answer, _) = split(Start::MayOpen, &["<thi"]);
    assert_eq!((reasoning.as_str(), answer.as_str()), ("", "<thi"));
}

fn template(source: &str) -> ChatTemplate {
    let vocab = VocabTokens {
        bos: "<s>".to_owned(),
        eos: "</s>".to_owned(),
        add_bos: false,
        add_eos: false,
    };
    ChatTemplate::new(source, vocab).expect("the template compiles")
}

/// One user turn, thinking on or off, `extra` keys laid over it.
fn chat(thinking: bool, extra: &Value) -> Value {
    let mut body = json!({
        "messages": [{"role": "user", "content": "And of Italy?"}],
        "chat_template_kwargs": {"enable_thinking": thinking},
    });
    for (key, value) in extra.as_object().expect("an object") {
        body[key] = value.clone();
    }
    body
}

fn start(source: &str, thinking: bool) -> Start {
    rendered(&chat(thinking, &json!({})), &template(source))
        .expect("the chat renders")
        .1
}

#[test]
fn each_pinned_template_starts_its_reply_where_llama_server_parses_it_from() {
    assert_eq!(start(QWEN35, true), Start::Reasoning);
    assert_eq!(start(QWEN35, false), Start::Answer);
    assert_eq!(start(QWEN3, true), Start::MayOpen);
    assert_eq!(start(QWEN3, false), Start::Answer);
    assert_eq!(start(GRANITE, true), Start::Answer);
}

#[test]
fn reasoning_format_none_leaves_the_reply_whole_and_a_format_not_made_here_is_refused() {
    let qwen35 = template(QWEN35);
    let none = rendered(&chat(true, &json!({"reasoning_format": "none"})), &qwen35);
    assert_eq!(none.expect("renders").1, Start::Answer);
    for format in ["auto", "deepseek"] {
        let split = rendered(&chat(true, &json!({"reasoning_format": format})), &qwen35);
        assert_eq!(split.expect("renders").1, Start::Reasoning);
    }
    for format in [json!("deepseek-legacy"), json!(true)] {
        let refused = rendered(&chat(true, &json!({"reasoning_format": format})), &qwen35);
        assert!(refused.is_err(), "{format}: {refused:?}");
    }
}
