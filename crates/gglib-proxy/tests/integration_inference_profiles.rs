//! End-to-end tests for `{model}:{profile}` request routing.
//!
//! These assert on the body the **upstream actually receives**, not on
//! internal state: the mock upstream records the forwarded JSON and each test
//! inspects the sampling parameters in it. That is the only thing that
//! determines how llama-server samples, so it is the only thing worth pinning.
//!
//! The proxy is the real `gglib_proxy::serve` with mock ports, as in
//! `integration_proxy_pipeline.rs`.
//!
//! The mock ports and the spawned-proxy harness live in
//! `fixtures/profile_mocks.rs` and `fixtures/profile_harness.rs` so that a
//! second suite can drive the same proxy without duplicating them.

use serde_json::{Value, json};

use gglib_core::domain::InferenceConfig;

mod fixtures;
use fixtures::profile_harness::{
    assert_deferred, assert_param, chat_request, coding_profile, spawn,
};
use fixtures::profile_mocks::MODEL;

// ─── Tests ─────────────────────────────────────────────────────────────────

/// The core behaviour: the suffix selects the profile, and its temperature is
/// what reaches llama-server.
#[tokio::test]
async fn profile_suffix_applies_its_sampling() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    let resp = h.post(chat_request("qwen:coding")).await;
    assert_eq!(resp.status(), 200);

    assert_param(&h.only_forwarded(), "temperature", 0.2);
}

/// The suffix must not reach the runtime: a profile selects sampling, never a
/// different model. If `qwen:coding` were launched as its own model it would
/// spawn a second llama-server and discard the KV cache on every switch.
///
/// The `model` field *inside* the forwarded body is deliberately left as the
/// client wrote it. llama-server ignores it (it serves whichever model is
/// loaded), and echoing it back unchanged means the client sees the id it
/// asked for rather than a silently rewritten one.
#[tokio::test]
async fn profile_suffix_is_stripped_before_the_model_is_launched() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    h.post(chat_request("qwen:coding")).await;

    assert_eq!(&*h.launched.lock().unwrap(), &[MODEL.to_owned()]);
    assert_eq!(
        h.only_forwarded().get("model").and_then(Value::as_str),
        Some("qwen:coding"),
        "the client's requested id passes through untouched"
    );
}

/// A bare model name must behave exactly as before profiles existed.
#[tokio::test]
async fn bare_model_name_is_unaffected_by_a_configured_profile() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    h.post(chat_request(MODEL)).await;

    // 0.7 is the hardcoded fallback — the profile's 0.2 must not leak in.
    assert_param(&h.only_forwarded(), "temperature", 0.7);
}

/// When the client is trusted (`Settings.trust_client_sampling: true` — an
/// explicit choice by whoever operates this proxy, not the default), the
/// client's own parameters sit above the profile in the hierarchy.
#[tokio::test]
async fn a_trusted_clients_temperature_beats_the_profile() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, true).await;

    let mut body = chat_request("qwen:coding");
    body["temperature"] = json!(1.5);
    h.post(body).await;

    assert_param(&h.only_forwarded(), "temperature", 1.5);
}

/// The default (`trust_client_sampling` unset). A client's own `temperature`
/// must NOT beat the profile — it is dropped from the hierarchy entirely, so
/// the profile applies exactly as if the client had sent no sampling
/// parameters at all. This is the actual end-to-end fix for a client that
/// hardcodes a sampling value with no user-facing control behind it (VS Code
/// Copilot's LLM Gateway sends `temperature: 0` on every request).
#[tokio::test]
async fn an_untrusted_clients_temperature_does_not_beat_the_profile() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    let mut body = chat_request("qwen:coding");
    body["temperature"] = json!(1.5);
    h.post(body).await;

    assert_param(&h.only_forwarded(), "temperature", 0.2); // the profile's own value
}

/// The invariant that makes one global profile safe across models: a
/// parameter the profile leaves unset still resolves from the model's own
/// stored defaults rather than being erased.
#[tokio::test]
async fn sparse_profile_leaves_model_defaults_intact() {
    let model_defaults = InferenceConfig {
        temperature: Some(1.0),
        top_p: Some(0.87),
        top_k: Some(20),
        ..Default::default()
    };
    let h = spawn(
        vec![coding_profile()],
        &[MODEL],
        Some(model_defaults),
        false,
    )
    .await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    // Profile wins where it speaks; the model default survives where it is silent.
    assert_param(&body, "temperature", 0.2);
    assert_param(&body, "top_p", 0.87);
    assert_param(&body, "top_k", 20.0);
}

/// Regression for #621, end to end: a `:coding` request must not reach the
/// upstream carrying a `presence_penalty` the model tuned for its own, much
/// higher, temperature. This is the exact request shape that failed in
/// production.
#[tokio::test]
async fn sparse_profile_does_not_forward_model_penalties() {
    let model_defaults = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    let h = spawn(
        vec![coding_profile()],
        &[MODEL],
        Some(model_defaults),
        false,
    )
    .await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    assert_param(&body, "temperature", 0.2);
    assert_deferred(&body, "presence_penalty");

    // The end-to-end shape ADR 0003 produces, asserted here because this is
    // the only test that sees a real forwarded body: one sampler, plus the
    // pipeline's own `cache_prompt`. Anything else is gglib overriding an
    // upstream default on a request nobody tuned.
    let mut keys: Vec<_> = body
        .as_object()
        .expect("a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["cache_prompt", "messages", "model", "stream", "temperature"],
        "{body}"
    );
}

/// A profile that was renamed or deleted must fail loudly, and must not reach
/// the upstream at all — a silently un-profiled request is the failure this
/// feature exists to prevent.
#[tokio::test]
async fn unknown_profile_suffix_is_rejected_without_calling_the_upstream() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    let resp = h.post(chat_request("qwen:codeing")).await;
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.expect("error body is JSON");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("profile_not_found"),
        "unexpected body: {body}"
    );
    // The message has to be actionable: it names the bad suffix and what exists.
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("codeing"), "unexpected message: {message}");
    assert!(message.contains("coding"), "unexpected message: {message}");

    assert!(
        h.forwarded.lock().unwrap().is_empty(),
        "rejected request must never reach the upstream"
    );
    assert!(
        h.launched.lock().unwrap().is_empty(),
        "rejected request must not launch a model"
    );
}

/// A model whose real name contains a colon keeps working, and is not
/// reinterpreted as a profile reference.
#[tokio::test]
async fn colon_bearing_model_name_still_resolves() {
    let h = spawn(vec![coding_profile()], &["qwen:27b"], None, false).await;

    let resp = h.post(chat_request("qwen:27b")).await;
    assert_eq!(resp.status(), 200);

    assert_eq!(&*h.launched.lock().unwrap(), &["qwen:27b".to_owned()]);
}

/// Regression guard for the dropped `max_tokens` fallback: with nothing
/// setting it, no cap may be forwarded.
#[tokio::test]
async fn no_max_tokens_is_forwarded_when_nothing_sets_one() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    assert!(
        body.get("max_tokens").is_none_or(Value::is_null),
        "unexpected max_tokens in forwarded body: {body}"
    );
}

/// A profile the user opted into listing shows up as its own picker entry,
/// alongside — not instead of — the bare model.
#[tokio::test]
async fn opted_in_profiles_are_listed_alongside_the_bare_model() {
    let mut listed = coding_profile();
    listed.list_in_models = true;
    let h = spawn(vec![listed], &[MODEL], None, false).await;

    let body = h.models().await;
    let ids: Vec<&str> = body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    assert!(ids.contains(&MODEL), "bare model missing from {ids:?}");
    assert!(ids.contains(&"qwen:coding"), "variant missing from {ids:?}");
    assert_eq!(
        ids.iter().filter(|id| **id == MODEL).count(),
        1,
        "bare model must still be listed exactly once: {ids:?}"
    );
}

/// An advertised variant must actually work when a client selects it — the
/// listing and the routing have to agree.
#[tokio::test]
async fn an_advertised_variant_can_be_selected_and_used() {
    let mut listed = coding_profile();
    listed.list_in_models = true;
    let h = spawn(vec![listed], &[MODEL], None, false).await;

    let ids: Vec<String> = h.models().await["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_owned))
        .collect();
    let advertised = ids
        .iter()
        .find(|id| id.contains(':'))
        .expect("a variant is advertised");

    let resp = h.post(chat_request(advertised)).await;
    assert_eq!(resp.status(), 200, "advertised id {advertised} must work");
    assert_param(&h.only_forwarded(), "temperature", 0.2);
}

/// With no profile opted in, the listing is exactly what it was before this
/// feature existed.
#[tokio::test]
async fn unlisted_profiles_do_not_appear_in_the_model_list() {
    let h = spawn(vec![coding_profile()], &[MODEL], None, false).await;

    let body = h.models().await;
    let ids: Vec<&str> = body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    assert!(
        !ids.contains(&"qwen:coding"),
        "unlisted profile leaked into {ids:?}"
    );
}
