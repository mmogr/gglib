//! A chat request read as llama-server reads it, and rendered.

use gglib_gguf::{ChatTemplate, VocabTokens};
use serde_json::{Value, json};

use super::Chat;

/// Qwen3-0.6B's template at the pin, as gglib-gguf's parity test holds it.
const QWEN3: &str = include_str!("../../gglib-gguf/src/render/testdata/Qwen-Qwen3-0.6B.jinja");

/// What the pinned llama-server rendered for the recorder's fixed chat with
/// thinking off.
const NO_THINKING: &str = include_str!(
    "../../../contracts/upstream/llama/bodies/apply-template.Qwen-Qwen3-0.6B.no-thinking.txt"
);

fn qwen3() -> ChatTemplate {
    let vocab = VocabTokens {
        bos: "<s>".to_owned(),
        eos: "</s>".to_owned(),
        add_bos: true,
        add_eos: false,
    };
    ChatTemplate::new(QWEN3, vocab).expect("the template compiles")
}

/// The recorder's fixed chat, with `extra` keys laid over it.
fn fixed_chat(extra: &Value) -> Value {
    let mut body = json!({"messages": [
        {"role": "system", "content": "You answer in one short sentence."},
        {"role": "user", "content": "What is the capital of France?"},
        {"role": "assistant", "content": "The capital of France is Paris."},
        {"role": "user", "content": "And of Italy?"},
    ]});
    for (key, value) in extra.as_object().expect("an object") {
        body[key] = value.clone();
    }
    body
}

fn rendered(body: &Value) -> String {
    Chat::read(body)
        .and_then(|chat| chat.render(&qwen3()))
        .expect("the chat renders")
}

#[test]
fn a_reasoning_budget_of_zero_renders_as_the_pin_renders_thinking_off() {
    let body = fixed_chat(&json!({"reasoning_budget_tokens": 0}));
    assert_eq!(rendered(&body), NO_THINKING);
    let asked = fixed_chat(&json!({"chat_template_kwargs": {"enable_thinking": false}}));
    assert_eq!(rendered(&asked), NO_THINKING);
    assert_ne!(rendered(&fixed_chat(&json!({}))), NO_THINKING);
}

#[test]
fn text_parts_are_joined_with_a_newline() {
    let parts = json!({"messages": [{"role": "user", "content": [
        {"type": "text", "text": "one"},
        {"type": "text", "text": "two"},
    ]}]});
    let whole = json!({"messages": [{"role": "user", "content": "one\ntwo"}]});
    assert_eq!(rendered(&parts), rendered(&whole));
}

#[test]
fn no_generation_prompt_is_opened_when_none_is_asked_for() {
    let open = rendered(&fixed_chat(&json!({})));
    let closed = rendered(&fixed_chat(&json!({"add_generation_prompt": false})));
    assert!(open.ends_with("<|im_start|>assistant\n"), "{open:?}");
    assert_eq!(open.strip_prefix(&closed), Some("<|im_start|>assistant\n"));
}

#[test]
fn a_request_without_messages_is_refused_as_llama_server_refuses_it() {
    assert_eq!(
        Chat::read(&json!({})).map(|_| ()),
        Err("'messages' is required".to_owned())
    );
    assert_eq!(
        Chat::read(&json!({"messages": "hi"})).map(|_| ()),
        Err("Expected 'messages' to be an array".to_owned())
    );
}
