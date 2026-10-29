//! Every prompt the pinned llama-server rendered for the recorder's fixed
//! chat, rendered again here and compared byte for byte.
//!
//! The bodies are what `POST /apply-template` answered at the pin
//! (`crates/gglib-runtime/src/engine_contract_readings.rs`, `fixed_chat`),
//! with stories260K loaded and each template given as a file. When a pin
//! bump records them again, `testdata/` moves to the new pin's templates in
//! the same pull request, or this fails, which is the signal that the
//! renders have to be read again.

use serde_json::json;

use super::{ChatKwargs, ChatMessage, ChatTemplate, VocabTokens, render_chat};

const BODIES: &str = "../../../../contracts/upstream/llama/bodies";

/// Each pinned template, with what the pin rendered for the fixed chat with
/// thinking on, then off.
const RECORDED: [(&str, &str, &str, &str); 3] = [
    (
        "Qwen3.5-4B",
        include_str!("testdata/Qwen3.5-4B.jinja"),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.Qwen3.5-4B.thinking.txt"
        ),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.Qwen3.5-4B.no-thinking.txt"
        ),
    ),
    (
        "Qwen-Qwen3-0.6B",
        include_str!("testdata/Qwen-Qwen3-0.6B.jinja"),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.Qwen-Qwen3-0.6B.thinking.txt"
        ),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.Qwen-Qwen3-0.6B.no-thinking.txt"
        ),
    ),
    (
        "ibm-granite-granite-4.0",
        include_str!("testdata/ibm-granite-granite-4.0.jinja"),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.ibm-granite-granite-4.0.thinking.txt"
        ),
        include_str!(
            "../../../../contracts/upstream/llama/bodies/apply-template.ibm-granite-granite-4.0.no-thinking.txt"
        ),
    ),
];

/// stories260K's vocabulary, which the recording loaded: `bos_token` and
/// `eos_token` as its `/props` answered (`props.<template>.json`), and the
/// BOS a sentencepiece vocabulary adds itself.
fn stories260k() -> VocabTokens {
    VocabTokens {
        bos: "<s>".to_owned(),
        eos: "</s>".to_owned(),
        add_bos: true,
        add_eos: false,
    }
}

/// The recorder's fixed chat.
fn fixed_chat() -> Vec<ChatMessage> {
    vec![
        ChatMessage::new("system", "You answer in one short sentence."),
        ChatMessage::new("user", "What is the capital of France?"),
        ChatMessage::new("assistant", "The capital of France is Paris."),
        ChatMessage::new("user", "And of Italy?"),
    ]
}

/// Which of the two recorded renders a request is held to.
#[derive(Clone, Copy)]
enum Variant {
    Thinking,
    NoThinking,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Thinking => "thinking",
            Self::NoThinking => "no-thinking",
        }
    }
}

/// What `body` renders to under each pinned template, beside what the pin
/// rendered as `variant`.
fn renders(body: &serde_json::Value, variant: Variant) -> Vec<(String, String, &'static str)> {
    let kwargs = ChatKwargs::from_request(body).expect("the request is valid");
    RECORDED
        .iter()
        .map(|&(name, source, thinking, no_thinking)| {
            let template = ChatTemplate::new(source, stories260k()).expect("compiles");
            let rendered = render_chat(&template, &fixed_chat(), true, &kwargs)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let recorded = match variant {
                Variant::Thinking => thinking,
                Variant::NoThinking => no_thinking,
            };
            (name.to_owned(), rendered, recorded)
        })
        .collect()
}

/// Every mismatch, both sides in full, so a failure carries its diff.
fn assert_all_equal(variant: Variant, results: &[(String, String, &str)]) {
    let mismatches: Vec<String> = results
        .iter()
        .filter(|(_, rendered, recorded)| rendered != recorded)
        .map(|(name, rendered, recorded)| {
            format!(
                "{BODIES}/apply-template.{name}.{}.txt\n  recorded: {recorded:?}\n  rendered: {rendered:?}",
                variant.name()
            )
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "render_chat differs from the pinned llama-server:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn every_template_renders_what_the_pin_rendered_with_thinking_on() {
    let body = json!({"chat_template_kwargs": {"enable_thinking": true}});
    assert_all_equal(Variant::Thinking, &renders(&body, Variant::Thinking));
}

#[test]
fn every_template_renders_what_the_pin_rendered_with_thinking_off() {
    let body = json!({"chat_template_kwargs": {"enable_thinking": false}});
    assert_all_equal(Variant::NoThinking, &renders(&body, Variant::NoThinking));
}

/// gglib's own rule, held to the recording: a budget of zero is the
/// no-thinking prompt, with no kwargs asked.
#[test]
fn a_reasoning_budget_of_zero_renders_the_pins_no_thinking_prompt() {
    let body = json!({"reasoning_budget_tokens": 0});
    assert_all_equal(Variant::NoThinking, &renders(&body, Variant::NoThinking));
}

/// A request that asks nothing renders as llama-server renders it with
/// thinking on, its default for a template with a reasoning block.
#[test]
fn a_request_that_asks_nothing_renders_the_pins_thinking_prompt() {
    assert_all_equal(Variant::Thinking, &renders(&json!({}), Variant::Thinking));
}
