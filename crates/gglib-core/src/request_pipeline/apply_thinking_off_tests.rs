//! Tests for [`super::apply`]: a request whose thinking budget resolves to
//! `0` tells the template that thinking is off, beside the budget, and no
//! other request is sent anything new.

use super::*;
use crate::domain::InferenceConfig;
use serde_json::json;

/// A question, with `extra` laid over it.
fn asking(extra: &Value) -> Value {
    let mut body =
        json!({"messages": [{"role": "user", "content": "What is the capital of France?"}]});
    for (key, value) in extra.as_object().expect("an object") {
        body[key] = value.clone();
    }
    body
}

/// `body` as the pipeline sends it, with `layers` beneath it.
fn shaped_with(mut body: Value, layers: &SamplingLayers) -> Value {
    apply(&mut body, &ModelContext::passthrough(), layers, None).expect("applies");
    body
}

/// `body` as the pipeline sends it, as gglib's own callers fold it.
fn shaped(body: Value) -> Value {
    let trusted = SamplingLayers {
        trust_client_sampling: true,
        ..SamplingLayers::default()
    };
    shaped_with(body, &trusted)
}

#[test]
fn a_budget_of_zero_tells_the_template_that_thinking_is_off() {
    let sent = shaped(asking(&json!({"reasoning_budget_tokens": 0})));

    assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
    assert_eq!(
        sent["chat_template_kwargs"],
        json!({"enable_thinking": false})
    );
}

/// Read from the budget the ladder resolved, wherever it came from: an
/// untrusted client's own, its alias, or a layer beneath the client.
#[test]
fn a_budget_of_zero_from_any_rung_or_spelling_tells_the_template_too() {
    let untrusted = SamplingLayers::default();
    let global_off = SamplingLayers {
        global: Some(InferenceConfig {
            reasoning_budget_tokens: Some(0),
            ..InferenceConfig::default()
        }),
        ..SamplingLayers::default()
    };
    for (body, layers) in [
        (json!({"reasoning_budget_tokens": 0}), &untrusted),
        (json!({"thinking_budget_tokens": 0}), &untrusted),
        (json!({}), &global_off),
    ] {
        let sent = shaped_with(asking(&body), layers);
        assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
        assert_eq!(
            sent["chat_template_kwargs"]["enable_thinking"],
            json!(false),
            "{sent}"
        );
    }
}

/// The client's other kwargs are kept; its own `enable_thinking: true` gives
/// way to its budget of `0`, as the in-process engine renders that request.
#[test]
fn a_clients_own_kwargs_are_merged_and_its_thinking_on_gives_way() {
    let sent = shaped(asking(&json!({
        "reasoning_budget_tokens": 0,
        "chat_template_kwargs": {"enable_thinking": true, "preserve_reasoning": false},
    })));

    assert_eq!(
        sent["chat_template_kwargs"],
        json!({"enable_thinking": false, "preserve_reasoning": false})
    );
}

/// A `null` is no kwargs at all, as llama-server reads it.
#[test]
fn null_template_kwargs_are_none_and_the_flag_is_sent() {
    let sent = shaped(asking(&json!({
        "reasoning_budget_tokens": 0,
        "chat_template_kwargs": null,
    })));

    assert_eq!(
        sent["chat_template_kwargs"],
        json!({"enable_thinking": false})
    );
}

/// Kwargs that are not an object are the client's mistake, and llama-server
/// answers it: gglib does not rewrite them into something else.
#[test]
fn template_kwargs_that_are_not_an_object_are_left_for_llama_server() {
    let sent = shaped(asking(&json!({
        "reasoning_budget_tokens": 0,
        "chat_template_kwargs": "enable_thinking=false",
    })));

    assert_eq!(sent["chat_template_kwargs"], json!("enable_thinking=false"));
}

/// Thinking on, by a budget or by saying nothing, sends the template nothing
/// of gglib's, `enable_thinking: true` included: the template's own default
/// stands. A client's own kwargs go as they came.
#[test]
fn thinking_on_or_unsaid_sends_the_template_nothing_new() {
    for extra in [
        json!({}),
        json!({"reasoning_budget_tokens": 512}),
        json!({"reasoning_budget_tokens": -1}),
    ] {
        let sent = shaped(asking(&extra));
        assert!(sent.get("chat_template_kwargs").is_none(), "{sent}");
    }

    let own = json!({"chat_template_kwargs": {"enable_thinking": false, "x": 1}});
    let sent = shaped(asking(&own));
    assert_eq!(sent["chat_template_kwargs"], own["chat_template_kwargs"]);

    let on =
        json!({"reasoning_budget_tokens": 512, "chat_template_kwargs": {"enable_thinking": true}});
    let sent = shaped(asking(&on));
    assert_eq!(sent["chat_template_kwargs"], on["chat_template_kwargs"]);
}

/// A budget the ladder replaces is the ladder's: an operator's flag above the
/// client's `0` turns nothing off.
#[test]
fn a_zero_the_ladder_overrules_tells_the_template_nothing() {
    let operator = SamplingLayers {
        cli_override: Some(InferenceConfig {
            reasoning_budget_tokens: Some(512),
            ..InferenceConfig::default()
        }),
        ..SamplingLayers::default()
    };
    let sent = shaped_with(asking(&json!({"reasoning_budget_tokens": 0})), &operator);

    assert_eq!(sent["reasoning_budget_tokens"], 512, "{sent}");
    assert!(sent.get("chat_template_kwargs").is_none(), "{sent}");
}
