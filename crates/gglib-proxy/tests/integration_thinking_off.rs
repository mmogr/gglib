//! A turn with Thinking off through the proxy: what llama-server is sent.
//!
//! gglib's own turns reach the proxy as a client's request does, with the
//! person's choice written as `reasoning_budget_tokens: 0` (the completion
//! adapter shapes nothing it sends through a door), so the request the proxy
//! forwards is the one the model sees. A budget of `0` alone ends the
//! thinking at llama-server's sampler while the template still asks for it,
//! and a hybrid model then reasons in its reply; the template is told too,
//! with `chat_template_kwargs.enable_thinking: false`. The recordings that
//! show both are `gglib-runtime`'s `tests/thinking_off_recordings.rs`.

use serde_json::{Value, json};

mod fixtures;
use fixtures::profile_harness::{Harness, spawn};
use fixtures::profile_mocks::MODEL;

/// A proxy for [`MODEL`] whose clients' sampling is not trusted: the default,
/// under which a client's budget is still its own.
async fn proxy() -> Harness {
    spawn(Vec::new(), &[MODEL], None, false).await
}

/// A question to [`MODEL`], with `extra` laid over it.
fn asking(extra: &Value) -> Value {
    let mut body = json!({
        "model": MODEL,
        "stream": false,
        "messages": [{"role": "user", "content": "What is the capital of France?"}],
    });
    for (key, value) in extra.as_object().expect("an object") {
        body[key] = value.clone();
    }
    body
}

/// The body llama-server is sent for `extra`.
async fn forwarded(extra: &Value) -> Value {
    let proxy = proxy().await;
    let response = proxy.post(asking(extra)).await;
    assert_eq!(response.status(), 200);
    proxy.only_forwarded()
}

#[tokio::test]
async fn a_thinking_off_turn_reaches_llama_server_with_the_template_told() {
    let sent = forwarded(&json!({"reasoning_budget_tokens": 0})).await;

    assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
    assert_eq!(
        sent["chat_template_kwargs"],
        json!({"enable_thinking": false}),
        "{sent}"
    );
}

/// A client's own kwargs are kept, and its `enable_thinking: true` beside a
/// budget of `0` gives way: that pair is the request that leaks.
#[tokio::test]
async fn a_clients_own_template_kwargs_are_kept_and_its_thinking_on_gives_way() {
    let sent = forwarded(&json!({
        "reasoning_budget_tokens": 0,
        "chat_template_kwargs": {"enable_thinking": true, "preserve_reasoning": false},
    }))
    .await;

    assert_eq!(
        sent["chat_template_kwargs"],
        json!({"enable_thinking": false, "preserve_reasoning": false}),
        "{sent}"
    );
}

/// Thinking on, by a budget or by saying nothing, sends no kwarg of gglib's:
/// the template's own default stands. A client's own kwargs go as they came.
#[tokio::test]
async fn thinking_on_or_unsaid_sends_the_template_nothing_new() {
    for extra in [
        json!({}),
        json!({"reasoning_budget_tokens": 512}),
        json!({"reasoning_budget_tokens": -1}),
    ] {
        let sent = forwarded(&extra).await;
        assert!(sent.get("chat_template_kwargs").is_none(), "{sent}");
    }

    let own = json!({"chat_template_kwargs": {"enable_thinking": false}});
    let sent = forwarded(&own).await;
    assert_eq!(sent["chat_template_kwargs"], own["chat_template_kwargs"]);
    assert!(sent.get("reasoning_budget_tokens").is_none(), "{sent}");
}
