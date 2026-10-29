//! What the shim refuses, and how: each refusal in llama-server's error
//! envelope, and none of them reaching the decoder.

use serde_json::{Value, json};

use super::test_support::{N_CTX, ask, empty, json_of, running};
use crate::decoder::Script;

/// `body`'s `error.type`, after checking `status` is `want`.
fn error_type((status, body): (u16, String), want: u16) -> String {
    assert_eq!(status, want, "{body}");
    json_of(&body)["error"]["type"]
        .as_str()
        .unwrap_or_else(|| panic!("no error.type: {body}"))
        .to_owned()
}

#[tokio::test]
async fn a_grammar_or_tools_is_refused_before_anything_is_decoded() {
    let shim = running(Script::new(["no"])).await;
    let asks: [(&str, Value); 9] = [
        ("grammar", json!("root ::= \"a\"")),
        ("json_schema", json!({"type": "object"})),
        ("response_format", json!({"type": "json_object"})),
        (
            "tools",
            json!([{"type": "function", "function": {"name": "f"}}]),
        ),
        ("logit_bias", json!({"15043": -100})),
        ("samplers", json!(["top_k"])),
        ("dry_sequence_breakers", json!(["\n"])),
        ("logprobs", json!(true)),
        ("n_probs", json!(3)),
    ];
    for (key, value) in asks {
        for stream in [false, true] {
            let mut body = ask("Hi");
            body[key] = value.clone();
            body["stream"] = json!(stream);
            let answer = shim.post("/v1/chat/completions", &body).await;
            assert_eq!(error_type(answer, 400), "invalid_request_error", "{key}");
        }
    }
    assert!(
        shim.record().prompts.is_empty(),
        "a refused request was decoded"
    );
}

#[tokio::test]
async fn a_plain_response_format_and_empty_tools_are_not_refused() {
    let shim = running(Script::new(["ok"])).await;
    let mut body = ask("Hi");
    body["response_format"] = json!({"type": "text"});
    body["tools"] = json!([]);
    body["grammar"] = json!("");
    assert_eq!(shim.post("/v1/chat/completions", &body).await.0, 200);
}

#[tokio::test]
async fn a_tool_turn_or_a_picture_in_the_history_is_refused() {
    let shim = running(Script::new(["no"])).await;
    let histories = [
        json!([{"role": "tool", "content": "42", "tool_call_id": "a"}]),
        json!([{"role": "assistant", "content": null, "tool_calls": [{"id": "a"}]}]),
        json!([{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:,"}}]}]),
    ];
    for messages in histories {
        let body = json!({"messages": messages});
        let answer = shim.post("/v1/chat/completions", &body).await;
        assert_eq!(
            error_type(answer, 400),
            "invalid_request_error",
            "{messages}"
        );
    }
}

#[tokio::test]
async fn a_prompt_that_fills_the_context_is_refused_as_llama_server_refuses_it() {
    let shim = running(Script::new(["no"])).await;
    let long = ask(&"word ".repeat(usize::try_from(N_CTX).unwrap()));
    for stream in [false, true] {
        let mut body = long.clone();
        body["stream"] = json!(stream);
        let (status, answer) = shim.post("/v1/chat/completions", &body).await;
        assert_eq!(status, 400, "{answer}");
        let error = &json_of(&answer)["error"];
        assert_eq!(error["type"], json!("exceed_context_size_error"));
        assert_eq!(error["code"], json!(400));
        assert_eq!(error["n_ctx"], json!(N_CTX));
        assert!(error["n_prompt_tokens"].as_u64().unwrap() >= u64::from(N_CTX));
    }
    assert!(
        shim.record().samplings.is_empty(),
        "an over-long prompt was decoded"
    );
}

#[tokio::test]
async fn slots_answers_501_so_the_proxy_s_poller_stops() {
    let shim = running(Script::new(["no"])).await;
    assert_eq!(
        error_type(shim.get("/slots").await, 501),
        "not_supported_error"
    );
    let save = shim.post("/slots/0?action=save", &json!({})).await;
    assert_eq!(error_type(save, 501), "not_supported_error");
}

#[tokio::test]
async fn what_the_shim_does_not_serve_is_llama_server_s_404() {
    let shim = running(Script::new(["no"])).await;
    assert_eq!(
        error_type(shim.get("/metrics").await, 404),
        "not_found_error"
    );
    assert_eq!(
        error_type(shim.get("/v1/chat/completions").await, 404),
        "not_found_error"
    );
}

#[tokio::test]
async fn with_no_model_held_the_shim_answers_that_it_is_loading() {
    let shim = empty(Script::new(["no"])).await;
    assert_eq!(
        error_type(shim.get("/health").await, 503),
        "unavailable_error"
    );
    assert_eq!(
        error_type(shim.get("/props").await, 503),
        "unavailable_error"
    );
    let chat = shim.post("/v1/chat/completions", &ask("Hi")).await;
    assert_eq!(error_type(chat, 503), "unavailable_error");
}

#[tokio::test]
async fn an_unloaded_model_is_no_longer_served() {
    let shim = running(Script::new(["ok"])).await;
    assert_eq!(shim.get("/health").await.0, 200);
    shim.engine.unload();
    assert_eq!(
        error_type(shim.get("/health").await, 503),
        "unavailable_error"
    );
    shim.shim.shutdown().await.expect("the shim stops");
}
