//! End-to-end tests for a pinned endpoint's default inference profile.
//!
//! `gglib serve --profile chat` makes requests that name the bare pinned model
//! resolve as if they had asked for `{model}:chat`. As with the sibling suite,
//! these assert on the body the **upstream actually receives** — that is what
//! determines how llama-server samples, so it is the only thing worth pinning.

use gglib_core::domain::{InferenceConfig, InferenceProfile};

mod fixtures;
use fixtures::profile_harness::{
    assert_param, chat_request, coding_profile, spawn_with_default_profile,
};
use fixtures::profile_mocks::MODEL;

fn creative_profile() -> InferenceProfile {
    InferenceProfile {
        name: "creative".to_owned(),
        description: None,
        config: InferenceConfig {
            temperature: Some(1.1),
            ..Default::default()
        },
        list_in_models: false,
    }
}

/// The feature: a bare model name picks up the endpoint's default.
#[tokio::test]
async fn a_default_profile_applies_to_a_bare_model_name() {
    let h = spawn_with_default_profile(
        vec![coding_profile()],
        &[MODEL],
        None,
        false,
        Some("coding".to_owned()),
    )
    .await;

    let resp = h.post(chat_request(MODEL)).await;
    assert_eq!(resp.status(), 200);

    assert_param(&h.only_forwarded(), "temperature", 0.2);
}

/// A client that names a profile still gets the one it named.
#[tokio::test]
async fn an_explicit_suffix_beats_the_default_profile() {
    let h = spawn_with_default_profile(
        vec![coding_profile(), creative_profile()],
        &[MODEL],
        None,
        false,
        Some("coding".to_owned()),
    )
    .await;

    let resp = h.post(chat_request(&format!("{MODEL}:creative"))).await;
    assert_eq!(resp.status(), 200);

    assert_param(&h.only_forwarded(), "temperature", 1.1);
}

/// The no-reload property this design rests on. A profile changes only the
/// request body, so a default must never reach the launch — if it did,
/// switching profiles would restart llama-server and drop the KV cache. This
/// is the test that catches someone "simplifying" the feature into
/// `plan_pinned_launch`.
#[tokio::test]
async fn the_default_profile_does_not_reach_the_launch() {
    let h = spawn_with_default_profile(
        vec![coding_profile()],
        &[MODEL],
        None,
        false,
        Some("coding".to_owned()),
    )
    .await;

    h.post(chat_request(MODEL)).await;

    let launched = h.launched.lock().unwrap().clone();
    assert_eq!(
        launched,
        vec![MODEL.to_owned()],
        "the launch must name the bare model, never a profile"
    );
}

/// A default must not become a fallback for a suffix the client got wrong.
/// They asked for something specific and can fix it; silence would sample at
/// the wrong temperature under a name they chose.
#[tokio::test]
async fn an_unknown_suffix_still_404s_with_a_default_configured() {
    let h = spawn_with_default_profile(
        vec![coding_profile()],
        &[MODEL],
        None,
        false,
        Some("coding".to_owned()),
    )
    .await;

    let resp = h.post(chat_request(&format!("{MODEL}:codeing"))).await;
    assert_eq!(resp.status(), 404);
    assert!(
        h.forwarded.lock().unwrap().is_empty(),
        "a rejected request must never reach the upstream"
    );
}

/// The asymmetry, stated deliberately: a *deleted default* degrades quietly
/// while a *named* suffix 404s. The client never asked for the default and
/// cannot fix it, so failing its request punishes the wrong party — the
/// operator hears about it in the proxy log instead.
#[tokio::test]
async fn a_deleted_default_profile_degrades_to_the_bare_resolution() {
    let h = spawn_with_default_profile(
        vec![coding_profile()],
        &[MODEL],
        None,
        false,
        Some("removed-since-launch".to_owned()),
    )
    .await;

    let resp = h.post(chat_request(MODEL)).await;
    assert_eq!(
        resp.status(),
        200,
        "the client's request must still succeed"
    );

    let body = h.only_forwarded();
    let applied = body.get("temperature").and_then(serde_json::Value::as_f64);
    assert_ne!(
        applied,
        Some(0.2),
        "the removed profile must not still be applied: {body}"
    );
}
