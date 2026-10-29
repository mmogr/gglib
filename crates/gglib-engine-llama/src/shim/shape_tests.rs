//! Every answer's key paths and JSON types, projected as the contract
//! recorder projects llama-server's, against what the pinned llama-server
//! answered (`contracts/upstream/llama/shape.txt`).
//!
//! The projection is the recorder's (`gglib-runtime`'s
//! `engine_contract_project.rs`), restated here: that crate is one a phone
//! build may not depend on.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::test_support::{N_CTX, ask, frames, json_of, running};
use crate::decoder::Script;

/// What the pinned llama-server answered, one `endpoint<TAB>path<TAB>type`
/// line each.
const RECORDED: &str = include_str!("../../../../contracts/upstream/llama/shape.txt");

/// Qwen3.5-4B's template at the pin: thinking on, its generation prompt
/// opens `<think>`.
const QWEN35: &str = include_str!("../../../gglib-gguf/src/render/testdata/Qwen3.5-4B.jinja");

/// The lines without which a projection says nothing: what gglib-proxy and
/// ggchat read.
const READ: [&str; 14] = [
    "/props\tdefault_generation_settings.n_ctx\tnumber",
    "/props\tbuild_info\tstring",
    "/props\tchat_template\tstring",
    "/health\tstatus\tstring",
    "/apply-template\tprompt\tstring",
    "chat completion\tchoices[].message.content\tstring",
    "chat completion\tchoices[].message.reasoning_content\tstring",
    "chat completion\tusage.prompt_tokens_details.cached_tokens\tnumber",
    "stream chunk\tprompt_progress.processed\tnumber",
    "stream chunk\tchoices[].finish_reason\tstring",
    "stream chunk\tchoices[].delta.reasoning_content\tstring",
    "stream chunk\tusage.total_tokens\tnumber",
    "stream chunk\ttimings.predicted_n\tnumber",
    "error (over context)\terror.n_ctx\tnumber",
];

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `value`'s key paths and types under `endpoint`, as the recorder writes
/// them: `a.b` for a key, `a[]` for an array's items.
fn shape(endpoint: &str, path: &str, value: &Value, out: &mut BTreeSet<String>) {
    if !path.is_empty() {
        out.insert(format!("{endpoint}\t{path}\t{}", type_name(value)));
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                shape(endpoint, &path, child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                shape(endpoint, &format!("{path}[]"), item, out);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn every_answer_s_shape_is_one_the_pinned_llama_server_answered() {
    let mut script = Script::new(["Hello", " there"]);
    script.batch = 4;
    let shim = running(script).await;
    let mut seen = BTreeSet::new();
    let mut project = |endpoint: &str, body: &Value| shape(endpoint, "", body, &mut seen);

    project("/props", &json_of(&shim.get("/props").await.1));
    project("/health", &json_of(&shim.get("/health").await.1));
    project(
        "/apply-template",
        &json_of(&shim.post("/apply-template", &ask("Hi")).await.1),
    );
    project(
        "chat completion",
        &json_of(&shim.post("/v1/chat/completions", &ask("Hi")).await.1),
    );
    for usage in [true, false] {
        let mut body = ask("Hi");
        body["stream"] = json!(true);
        body["return_progress"] = json!(true);
        body["stream_options"] = json!({"include_usage": usage});
        for frame in frames(&shim.post("/v1/chat/completions", &body).await.1) {
            project("stream chunk", &frame);
        }
    }
    // llama-server answers every error in one envelope, recorded under its
    // over-context answer: over context, streamed or not, a refusal, and
    // the 501 of `/slots`.
    let long = ask(&"word ".repeat(usize::try_from(N_CTX).unwrap()));
    for stream in [false, true] {
        let mut body = long.clone();
        body["stream"] = json!(stream);
        project(
            "error (over context)",
            &json_of(&shim.post("/v1/chat/completions", &body).await.1),
        );
    }
    let mut grammar = ask("Hi");
    grammar["grammar"] = json!("root ::= \"a\"");
    project(
        "error (over context)",
        &json_of(&shim.post("/v1/chat/completions", &grammar).await.1),
    );
    for slots in [
        shim.get("/slots").await,
        shim.post("/slots/0?action=save", &json!({})).await,
    ] {
        project("error (over context)", &json_of(&slots.1));
    }
    project(
        "error (over context)",
        &json_of(&shim.get("/metrics").await.1),
    );

    // A thinking model's reply, reasoning then answer.
    let mut script = Script::new(["Rome", " is.", "\n</think>\n\n", "Rome."]);
    script.chat_template = QWEN35.to_owned();
    let thinking = running(script).await;
    project(
        "chat completion",
        &json_of(&thinking.post("/v1/chat/completions", &ask("Hi")).await.1),
    );
    let mut body = ask("Hi");
    body["stream"] = json!(true);
    body["stream_options"] = json!({"include_usage": true});
    for frame in frames(&thinking.post("/v1/chat/completions", &body).await.1) {
        project("stream chunk", &frame);
    }

    let recorded: BTreeSet<&str> = RECORDED.lines().collect();
    let unseen: Vec<&String> = seen
        .iter()
        .filter(|line| !recorded.contains(line.as_str()))
        .collect();
    assert!(
        unseen.is_empty(),
        "lines the pinned llama-server never answered:\n{unseen:#?}"
    );
    let missing: Vec<&str> = READ
        .into_iter()
        .filter(|line| !seen.contains(*line))
        .collect();
    assert!(
        missing.is_empty(),
        "lines gglib reads, not answered: {missing:#?}"
    );
}
