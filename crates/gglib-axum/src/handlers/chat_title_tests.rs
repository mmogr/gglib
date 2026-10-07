//! Tests for [`super`]: what a title request may say, and the request the
//! model is sent for it.

use gglib_core::domain::{DefaultsOrigin, ModelCapabilities};
use serde_json::json;

use super::*;

/// What the page sends for a title, as JSON.
fn wire(messages: &Value) -> Value {
    json!({ "port": 9000, "messages": messages, "temperature": 0.7, "max_tokens": 20 })
}

fn asked(messages: &Value) -> ChatTitleRequest {
    serde_json::from_value(wire(messages)).expect("a title request")
}

fn one_question() -> Value {
    json!([{ "role": "user", "content": "What is a tabby?" }])
}

/// The request the model is sent for [`one_question`].
fn sent(ctx: &ModelContext, global: Option<InferenceConfig>) -> Value {
    model_request(asked(&one_question()), ctx, global).expect("a request")
}

/// A model whose stored defaults, set by its owner, name `defaults`.
fn model_with(defaults: InferenceConfig) -> ModelContext {
    ModelContext {
        inference_defaults: Some(defaults),
        defaults_origin: Some(DefaultsOrigin::User),
        ..ModelContext::passthrough()
    }
}

fn temperature_of(body: &Value) -> f64 {
    body["temperature"].as_f64().expect("a temperature")
}

fn roles_of(body: &Value) -> Vec<&str> {
    body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|m| m["role"].as_str().expect("a role"))
        .collect()
}

// ── What a title request may say ─────────────────────────────────────────────

#[test]
fn the_token_cap_the_page_sends_is_the_cap_the_model_is_sent() {
    let body = sent(&ModelContext::passthrough(), None);

    assert_eq!(body["max_tokens"], 20);
}

/// A title's cap and temperature are the request's own, over the model's and
/// the settings': a cap that is dropped lets theirs apply in its place.
#[test]
fn no_stored_default_outranks_what_a_title_request_says() {
    let model = model_with(InferenceConfig {
        max_tokens: Some(4096),
        temperature: Some(0.2),
        ..InferenceConfig::default()
    });
    let global = InferenceConfig {
        max_tokens: Some(2048),
        temperature: Some(1.0),
        ..InferenceConfig::default()
    };

    let body = sent(&model, Some(global));

    assert_eq!(body["max_tokens"], 20);
    assert!((temperature_of(&body) - 0.7).abs() < 1e-6, "{body}");
}

#[test]
fn a_title_is_asked_for_without_thinking_whatever_the_model_defaults_to() {
    let thinker = model_with(InferenceConfig {
        reasoning_budget_tokens: Some(8192),
        ..InferenceConfig::default()
    });

    assert_eq!(sent(&thinker, None)["reasoning_budget_tokens"], 0);
    assert_eq!(
        sent(&ModelContext::passthrough(), None)["reasoning_budget_tokens"],
        0
    );
}

#[test]
fn a_request_naming_any_other_key_is_refused() {
    for key in ["tools", "tool_choice", "stream", "maxTokens", "model"] {
        let mut body = wire(&one_question());
        body[key] = json!(true);

        let refused = serde_json::from_value::<ChatTitleRequest>(body).expect_err(key);

        assert!(refused.to_string().contains(key), "{key}: {refused}");
    }
}

#[test]
fn a_message_naming_tool_calls_is_refused() {
    let messages = json!([{ "role": "assistant", "content": "", "tool_calls": [] }]);

    let refused = serde_json::from_value::<ChatTitleRequest>(wire(&messages))
        .expect_err("a message with tool calls");

    assert!(refused.to_string().contains("tool_calls"), "{refused}");
}

#[test]
fn a_request_without_its_cap_or_its_temperature_is_refused() {
    for key in ["max_tokens", "temperature"] {
        let mut body = wire(&one_question());
        body.as_object_mut().expect("an object").remove(key);

        let refused = serde_json::from_value::<ChatTitleRequest>(body).expect_err(key);

        assert!(refused.to_string().contains(key), "{key}: {refused}");
    }
}

// ── The request the model is sent ────────────────────────────────────────────

#[test]
fn the_model_is_sent_no_tools_and_is_not_asked_to_stream() {
    let body = sent(&ModelContext::passthrough(), None);

    assert_eq!(body["stream"], false);
    assert!(body.get("tools").is_none(), "{body}");
    assert!(body.get("tool_choice").is_none(), "{body}");
}

#[test]
fn the_settings_still_fill_what_a_title_request_does_not_say() {
    let global = InferenceConfig {
        top_k: Some(40),
        ..InferenceConfig::default()
    };

    assert_eq!(
        sent(&ModelContext::passthrough(), Some(global))["top_k"],
        40
    );
    assert!(
        sent(&ModelContext::passthrough(), None)
            .get("top_k")
            .is_none()
    );
}

/// The pipeline's own work, seen on a title: a strict-turn model's two
/// messages in a row from one role are merged, and `cache_prompt` is pinned.
#[test]
fn the_request_is_shaped_as_a_turn_of_the_chat_is() {
    let strict = ModelContext {
        capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS
            | ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        ..ModelContext::passthrough()
    };
    let two_in_a_row = json!([
        { "role": "user", "content": "What is a tabby?" },
        { "role": "user", "content": "Title this." },
    ]);

    let shaped = model_request(asked(&two_in_a_row), &strict, None).expect("a request");
    let unshaped =
        model_request(asked(&two_in_a_row), &ModelContext::passthrough(), None).expect("a request");

    assert_eq!(roles_of(&shaped), ["user"]);
    assert_eq!(roles_of(&unshaped), ["user", "user"]);
    assert_eq!(shaped["cache_prompt"], true);
}

#[test]
fn a_message_with_no_text_is_left_out_and_a_tools_result_is_kept() {
    let messages = json!([
        { "role": "user", "content": "What is a tabby?" },
        { "role": "assistant", "content": "  \n" },
        { "role": "tool", "content": "" },
        { "role": "user", "content": "Title this." },
    ]);

    let body =
        model_request(asked(&messages), &ModelContext::passthrough(), None).expect("a request");

    assert_eq!(roles_of(&body), ["user", "tool", "user"]);
}

#[test]
fn a_conversation_with_no_text_at_all_is_a_400() {
    let messages = json!([{ "role": "assistant", "content": "" }]);

    let refused = model_request(asked(&messages), &ModelContext::passthrough(), None)
        .expect_err("nothing to send");

    assert!(
        matches!(&refused, HttpError::BadRequest(why) if why.contains("No valid messages")),
        "{refused:?}"
    );
}

#[test]
fn a_conversation_the_models_context_cannot_hold_is_a_400() {
    let small = ModelContext {
        context_length: Some(8),
        ..ModelContext::passthrough()
    };
    let long = json!([{ "role": "user", "content": "tabby ".repeat(200) }]);

    let refused = model_request(asked(&long), &small, None).expect_err("over the budget");
    let fits = model_request(asked(&long), &ModelContext::passthrough(), None);

    assert!(
        matches!(&refused, HttpError::BadRequest(why) if why.contains("context budget")),
        "{refused:?}"
    );
    assert!(fits.is_ok(), "an unknown context is not measured");
}
