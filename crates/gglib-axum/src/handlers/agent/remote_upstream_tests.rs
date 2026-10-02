//! Which upstream a request drives, and the model each turn is made by.

use super::*;

fn req(json: &str) -> AgentChatRequest {
    serde_json::from_str(json).expect("parses")
}

/// A body with `remote` and no `model` is refused here, rather than
/// arriving at the far proxy as `"model": ""`.
#[test]
fn a_remote_request_naming_no_model_is_refused_here() {
    let err = remote_model(&req(r#"{"port":9000,"messages":[],"remote":true}"#))
        .expect_err("no model named");
    assert!(matches!(err, HttpError::BadRequest(_)), "got {err:?}");
    assert!(
        err.to_string().contains("no model named"),
        "the message has to name the real problem, got: {err}"
    );
}

/// A field holding only spaces is the same absence, and `trim` downstream
/// would otherwise turn it into the same empty model.
#[test]
fn a_model_of_only_whitespace_is_no_model_at_all() {
    assert!(
        remote_model(&req(
            r#"{"port":9000,"messages":[],"remote":true,"model":"  "}"#
        ))
        .is_err()
    );
}

#[test]
fn a_named_model_is_forwarded_trimmed() {
    assert_eq!(
        remote_model(&req(
            r#"{"port":9000,"messages":[],"remote":true,"model":" qwen3 "}"#
        ))
        .expect("a name"),
        "qwen3"
    );
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

/// The same absence the remote path refuses outright, read the same way
/// here so the two cannot drift apart.
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
    let upstream = local(&state, &asked, server).await;
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
    let upstream = local(&state, &req(r#"{"port":9000,"messages":[]}"#), server).await;
    assert_eq!(upstream.made_by.quantization, None);
}

/// The far machine's catalogue is not this one's: a remote run is made by
/// the model it named, and has no quantisation.
#[test]
fn a_remote_run_is_made_by_the_named_model_with_no_quantisation() {
    let upstream = remote(
        "qwen3".to_owned(),
        "http://127.0.0.1:7000".to_owned(),
        FarMachine {
            key: "key".to_owned(),
            name: "desk".to_owned(),
        },
    );
    assert_eq!(upstream.made_by.model, "qwen3");
    assert_eq!(upstream.made_by.quantization, None);
}
