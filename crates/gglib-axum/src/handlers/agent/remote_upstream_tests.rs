//! Which upstream a request drives, and the model each turn is made by.

use super::*;
use crate::handlers::remote::fake_far::{FINGERPRINT, carries_key, far as fake_far, only};

fn req(json: &str) -> AgentChatRequest {
    serde_json::from_str(json).expect("parses")
}

/// The model the port is actually serving, for the three ways a local
/// request declines to name one.
fn running(name: &str) -> ServerInfo {
    ServerInfo {
        model_id: 1,
        model_name: name.to_owned(),
        pid: Some(4242),
        port: 9000,
        started_at: 0,
    }
}

/// Locally an absent model means "whatever is loaded", and that is a real
/// model with a real name — so the count goes under it rather than under a
/// placeholder, which would put real traffic in a bucket that is not a
/// model.
#[test]
fn a_request_naming_no_model_is_counted_under_the_running_one() {
    assert_eq!(
        counted_as(&req(r#"{"port":9000,"messages":[]}"#), &running("qwen3")),
        "qwen3"
    );
}

/// A name of only spaces is an absence, read as one.
#[test]
fn a_whitespace_model_name_is_no_name_at_all() {
    assert_eq!(
        counted_as(
            &req(r#"{"port":9000,"messages":[],"model":"   "}"#),
            &running("qwen3")
        ),
        "qwen3",
        "a name of only spaces is an absence, not a model called \"   \""
    );
}

/// And the fallback is a fallback: a request that named a model is counted
/// under that name even when the port is serving something else, because
/// the name the client chose is the key it will look the count up by.
///
/// Close to, but not identical with, what the proxy would key the same
/// traffic under. The proxy counts after `resolve_route`, so a
/// `model:profile` request lands under the base model, where this path
/// talks straight to llama-server and would count the suffixed string;
/// and this trims where the name that goes on the wire does not.
#[test]
fn a_named_model_is_counted_under_its_own_name() {
    assert_eq!(
        counted_as(
            &req(r#"{"port":9000,"messages":[],"model":" llama3 "}"#),
            &running("qwen3")
        ),
        "llama3"
    );
}

/// A model in this machine's catalogue, with `quantization`.
async fn catalogued(state: &AppState, quantization: Option<&str>) -> i64 {
    let mut model = gglib_core::domain::NewModel::new(
        "catalogue-name".to_owned(),
        std::path::PathBuf::from("/models/served.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    model.quantization = quantization.map(ToOwned::to_owned);
    state.core.models().add(model).await.expect("added").id
}

/// Each turn is made by the model the port serves, whatever the request
/// asked for, with the quantisation its catalogue entry names.
#[tokio::test]
async fn a_local_run_is_made_by_the_model_its_port_serves() {
    let (_dir, state) = super::super::run_fixture::state().await;
    let id = catalogued(&state, Some("Q4_K_M")).await;
    let server = ServerInfo {
        model_id: id,
        ..running("served-7b")
    };
    let asked = req(r#"{"port":9000,"messages":[],"model":"asked-for"}"#);
    let upstream = local(&state, &asked, server).await.unwrap();
    assert_eq!(upstream.made_by.model, "served-7b");
    assert_eq!(upstream.made_by.quantization.as_deref(), Some("Q4_K_M"));
}

/// A catalogue entry that names no quantisation gives none, not a guess.
#[tokio::test]
async fn a_local_model_without_a_quantisation_has_none() {
    let (_dir, state) = super::super::run_fixture::state().await;
    let id = catalogued(&state, None).await;
    let server = ServerInfo {
        model_id: id,
        ..running("served-7b")
    };
    let asked = req(r#"{"port":9000,"messages":[]}"#);
    let upstream = local(&state, &asked, server).await.unwrap();
    assert_eq!(upstream.made_by.quantization, None);
}

/// A local run is shaped for the model its port serves, by that model's
/// catalogue row, and not for a model the request's `model` happens to name:
/// that name goes on the wire and picks nothing here. Beneath it sit this
/// machine's global sampling defaults, and no profile.
#[tokio::test]
async fn a_local_run_is_shaped_for_the_model_its_port_serves_over_the_global_defaults() {
    let (_dir, state) = super::super::run_fixture::state().await;
    let tagged = |name: &str, tag: &str| {
        let path = std::path::PathBuf::from(format!("/models/{name}.gguf"));
        let mut model =
            gglib_core::domain::NewModel::new(name.to_owned(), path, 7.0, chrono::Utc::now());
        model.tags = vec![tag.to_owned()];
        model
    };
    let models = state.core.models();
    let on_port = models.add(tagged("served", "reasoning")).await.unwrap().id;
    models.add(tagged("asked-for", "agent")).await.unwrap();
    let global = gglib_core::domain::InferenceConfig {
        temperature: Some(0.42),
        ..Default::default()
    };
    let update = gglib_core::settings::SettingsUpdate {
        inference_defaults: Some(Some(global.clone())),
        ..Default::default()
    };
    state.core.settings().update(update).await.unwrap();
    let server = ServerInfo {
        model_id: on_port,
        ..running("served")
    };

    for body in [
        r#"{"port":9000,"messages":[]}"#,
        r#"{"port":9000,"messages":[],"model":"asked-for"}"#,
    ] {
        let asked = req(body);
        let upstream = local(&state, &asked, server.clone()).await.unwrap();
        assert!(upstream.model_context.catalog_resolved, "{body}");
        assert_eq!(upstream.model_context.tags, ["reasoning"], "{body}");
        assert_eq!(upstream.model, asked.model, "the name on the wire");
        let layers = SamplingLayers {
            global: Some(global.clone()),
            ..SamplingLayers::default()
        };
        assert_eq!(upstream.layers, layers, "{body}");
    }
}

/// A model of the paired machine, by its id there.
fn far_ref(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Paired {
            fingerprint: FINGERPRINT.to_owned(),
        },
        id,
    }
}

/// The far detail route's answer for model 3, as a gglib proxy writes it.
const LOOKUP: &str = r#"{"detail":{"id":3,"name":"org/qwen3","paramCountB":8.0,
    "quantization":"Q4_K_M","addedAt":"2026-10-01 09:00:00","isServing":false,"metadata":{}}}"#;

/// A ref to this machine names a model that is driven by its port: refused
/// before the connection is read, so with no tunnel up it is still a `400`.
#[tokio::test]
async fn a_far_ref_to_this_machine_is_refused() {
    let (_dir, state) = super::super::run_fixture::state().await;
    let body = r#"{"port":0,"messages":[],"far":{"machine":{"kind":"local"},"id":3}}"#;

    let Err(err) = resolve(&state, &req(body)).await else {
        panic!("a local ref was driven as a far model");
    };

    assert!(matches!(err, HttpError::BadRequest(_)), "got {err:?}");
}

/// The far model is looked up by its id, sent by its id, and counted and
/// made under the name and quantisation that machine has for it.
#[tokio::test]
async fn a_far_run_sends_the_id_and_is_made_by_the_far_name() {
    let (fake, far) = fake_far(200, LOOKUP).await;

    let upstream = remote(&far, &far_ref(3)).await.expect("looked up");

    assert_eq!(
        upstream.model.as_deref(),
        Some("3"),
        "the wire carries the id"
    );
    assert_eq!(upstream.counted_as, "org/qwen3");
    assert_eq!(upstream.made_by.model, "org/qwen3");
    assert_eq!(upstream.made_by.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(upstream.far_model, Some(far_ref(3)));
    assert_eq!(upstream.local_model, None);
    assert_eq!(upstream.base_url, far.server_root());
    // The far proxy shapes the turn and folds its own layers.
    assert_eq!(upstream.model_context, ModelContext::passthrough());
    assert_eq!(upstream.layers, SamplingLayers::default());
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", "/v1/models/3/detail")
    );
    assert!(carries_key(&seen), "{seen:?}");
}

/// An id the far machine does not have is its `404`, with its words, and
/// no turn starts.
#[tokio::test]
async fn a_far_id_that_machine_does_not_have_is_its_404() {
    let refusal = r#"{"error":{"message":"No model with that id or name is in the catalog: 9","code":"model_not_found"}}"#;
    let (_, far) = fake_far(404, refusal).await;

    let Err(err) = remote(&far, &far_ref(9)).await else {
        panic!("a missing model was driven");
    };

    let HttpError::NotFound(message) = err else {
        panic!("got {err:?}");
    };
    assert!(message.ends_with("in the catalog: 9"), "{message}");
}

/// A key that machine no longer admits is a `409` that says to pair again,
/// never a `401`, which would read as this daemon wanting a key.
#[tokio::test]
async fn a_far_key_refused_at_lookup_is_a_conflict() {
    let (_, far) = fake_far(401, r#"{"error":{"message":"Invalid API key"}}"#).await;

    let Err(err) = remote(&far, &far_ref(3)).await else {
        panic!("a refused key was driven");
    };

    assert!(
        matches!(&err, HttpError::Conflict(m) if m.contains("pair again")),
        "got {err:?}"
    );
}
