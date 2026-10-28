//! The prompt a chat is decoded from is the one the pinned llama-server
//! renders for it, over the wire.

use serde_json::{Value, json};

use super::test_support::{json_of, running};
use crate::decoder::Script;

/// Qwen3-0.6B's template at the pin, as gglib-gguf's parity test holds it.
const QWEN3: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen-Qwen3-0.6B.jinja");

/// What the pinned llama-server rendered for the recorder's fixed chat with
/// thinking on.
const THINKING: &str = include_str!(
    "../../../../contracts/upstream/llama/bodies/apply-template.Qwen-Qwen3-0.6B.thinking.txt"
);

/// The recorder's fixed chat, thinking on.
fn fixed_chat() -> Value {
    json!({
        "messages": [
            {"role": "system", "content": "You answer in one short sentence."},
            {"role": "user", "content": "What is the capital of France?"},
            {"role": "assistant", "content": "The capital of France is Paris."},
            {"role": "user", "content": "And of Italy?"},
        ],
        "chat_template_kwargs": {"enable_thinking": true},
    })
}

#[tokio::test]
async fn apply_template_and_the_decoded_prompt_are_the_pins_render() {
    let mut script = Script::new(["Rome."]);
    script.chat_template = QWEN3.to_owned();
    let shim = running(script).await;

    let (status, body) = shim.post("/apply-template", &fixed_chat()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(json_of(&body)["prompt"], json!(THINKING));

    shim.post("/v1/chat/completions", &fixed_chat()).await;
    // The vocabulary adds its BOS when the prompt is tokenized.
    assert_eq!(shim.record().prompts, vec![format!("<s>{THINKING}")]);

    let props = json_of(&shim.get("/props").await.1);
    assert_eq!(props["chat_template"], json!(QWEN3));
    assert_eq!(props["bos_token"], json!("<s>"));
}
