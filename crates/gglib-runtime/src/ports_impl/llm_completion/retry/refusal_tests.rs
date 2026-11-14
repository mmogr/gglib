//! What a turn's caller is told when the upstream refuses the key it sent,
//! or refuses the turn.
//!
//! One step past classification: not how a refusal is sorted, but which
//! sentence comes out of [`send_with_retry`] once it has been — and that the
//! answer depends on whether the machine that wrote it was another one.
//!
//! That dependence is the whole point, so the two upstreams are represented
//! by their real bodies rather than by one body used twice. gglib's own proxy
//! writes [`proxy_invalid_key_body`] and modelpipe's tunnel edge writes
//! [`edge_invalid_api_key_body`]; they share the `code` and nothing else. A
//! rewrite firing on the shared part alone would tell someone whose *local*
//! key is wrong to go and re-pair a machine that is not involved.

use std::time::Duration;

use gglib_core::ports::UpstreamRefusal;
use gglib_core::retry::RetryPolicy;
use reqwest::Client;

use super::super::Door;
use super::send_with_retry;
use super::test_server::{
    TestServer, edge_backend_unreachable_body, edge_invalid_api_key_body, json,
    proxy_invalid_key_body,
};

/// As `gglib remote status` prints it, and as the CLI's pre-turn banner has
/// just shown the user.
const NAME: &str = "desk";

fn paired_machine() -> Door {
    Door {
        root_url: String::new(),
        key: Some("the-key-stored-at-pairing".to_owned()),
        name: Some(NAME.to_owned()),
    }
}

/// Serve one refusal and return the message its caller ends up with.
async fn refused(status: u16, reason: &str, body: &str, far: Option<&Door>) -> String {
    refusal(status, reason, body, far).await.to_string()
}

/// Serve one refusal and return the error its caller ends up with.
async fn refusal(status: u16, reason: &str, body: &str, far: Option<&Door>) -> anyhow::Error {
    let server = TestServer::start(vec![json(status, reason, body)]).await;
    send_with_retry(
        &Client::new(),
        &format!("{}/v1/chat/completions", server.base_url),
        far,
        &serde_json::json!({"model": "test"}),
        Duration::from_secs(5),
        &RetryPolicy::default(),
        None,
    )
    .await
    .expect_err("a refusal is never a response to stream")
}

/// The defect this file exists for.
///
/// The stored key is not the far machine's current one — usually because that
/// machine rotated its `proxy_api_key`. What used to come back was
/// `401 Unauthorized invalid_api_key: invalid or missing bearer token`: a
/// sentence with no machine in it and nothing to do about it. Both halves of
/// the remedy are asserted, because either alone leaves the reader stuck —
/// the name says *which* machine to go to, `gglib remote enable` says
/// what to do once there.
#[tokio::test]
async fn a_far_machines_refused_key_names_the_machine_and_the_remedy() {
    let message = refused(
        401,
        "Unauthorized",
        &edge_invalid_api_key_body(),
        Some(&paired_machine()),
    )
    .await;

    assert!(
        message.starts_with(&format!("{NAME} is not admitting")),
        "the message must name the machine that refused: {message}"
    );
    assert!(
        message.contains("gglib remote invite"),
        "the message must say how to fix it: {message}"
    );
}

/// The scope guard, and the reason the rewrite cannot key on the code alone.
///
/// This machine's own proxy answers a bad key with a body of its own that
/// carries the same `invalid_api_key`. There is no far machine on that path
/// and no pairing to redo, so the caller must keep getting the classifier's
/// rendering — which, note, labels it by this proxy's `type`, with the code
/// the two bodies share after it.
#[tokio::test]
async fn this_proxys_own_refused_key_is_left_as_the_classifier_rendered_it() {
    let message = refused(401, "Unauthorized", &proxy_invalid_key_body(), None).await;

    assert_eq!(
        message,
        "401 Unauthorized invalid_request_error (invalid_api_key): Missing or invalid API key. \
         Send it as 'Authorization: Bearer <key>'.",
        "with no far machine there is nobody to name and nothing to re-pair"
    );
}

/// The same refusal through this machine's own door: the door was dialled
/// with the key its endpoint demands, so the sentence names no machine and
/// sends nobody to re-pair; it says to send again once a changed key has
/// reached the endpoint.
#[tokio::test]
async fn this_machines_door_says_its_own_sentence_for_a_refused_key() {
    let local = Door {
        root_url: String::new(),
        key: Some("this-machines-key".to_owned()),
        name: None,
    };
    let message = refused(401, "Unauthorized", &proxy_invalid_key_body(), Some(&local)).await;

    assert_eq!(
        message,
        "this machine's endpoint did not admit its own key — the key was changed a moment ago, \
         and the endpoint takes up to 5 seconds to pick up a new one. Send again in a few seconds"
    );
}

/// The other half of the scope guard: being remote is not on its own a reason
/// to blame the key.
///
/// A far machine whose model server is down fails terminally too, and telling
/// its owner to re-pair would send them to fix a credential that is working.
/// Only the code decides which sentence is right.
#[tokio::test]
async fn a_far_machines_other_failures_are_not_blamed_on_the_key() {
    let message = refused(
        502,
        "Bad Gateway",
        &edge_backend_unreachable_body(),
        Some(&paired_machine()),
    )
    .await;

    assert_eq!(
        message,
        "desk answered 502 Bad Gateway backend_unreachable: the serving side could not reach its \
         backend",
        "only a refused key is reported as a refused key"
    );
}

/// A paired machine's model that cannot read images is refused by that
/// machine's proxy, not here, and its refusal reaches the caller whole: the
/// code, kept as the error's own beside the sentence, and the sentence with
/// the model it names and the command that links a projector, after the
/// machine's name, since the command is to be run there.
#[tokio::test]
async fn a_far_machines_refusal_of_an_image_arrives_whole() {
    use gglib_core::request_pipeline::CannotReadImages;
    use gglib_proxy::models::ErrorResponse;

    let sentence = CannotReadImages.message("7");
    let body = serde_json::to_string(&ErrorResponse::with_code(
        sentence.clone(),
        "invalid_request_error",
        CannotReadImages.code(),
    ))
    .unwrap();

    let error = refusal(400, "Bad Request", &body, Some(&paired_machine())).await;

    let refusal = error
        .downcast_ref::<UpstreamRefusal>()
        .expect("a refusal with a code is typed");
    assert_eq!(
        (refusal.status, refusal.code.as_str()),
        (400, "model_cannot_read_images")
    );
    assert_eq!(
        error.to_string(),
        format!(
            "{NAME} answered 400 Bad Request invalid_request_error (model_cannot_read_images): \
             {sentence}"
        )
    );
}

/// The same refusal from this machine's endpoint names no machine.
#[tokio::test]
async fn this_machines_refusal_of_an_image_names_no_machine() {
    use gglib_core::request_pipeline::CannotReadImages;
    use gglib_proxy::models::ErrorResponse;

    let sentence = CannotReadImages.message("7");
    let body = serde_json::to_string(&ErrorResponse::with_code(
        sentence.clone(),
        "invalid_request_error",
        CannotReadImages.code(),
    ))
    .unwrap();
    let local = Door {
        root_url: String::new(),
        key: None,
        name: None,
    };

    let error = refusal(400, "Bad Request", &body, Some(&local)).await;

    assert_eq!(
        error
            .downcast_ref::<UpstreamRefusal>()
            .map(|r| r.code.as_str()),
        Some("model_cannot_read_images")
    );
    assert_eq!(
        error.to_string(),
        format!("400 Bad Request invalid_request_error (model_cannot_read_images): {sentence}")
    );
}

/// A paired machine still loading its model, retried until the policy gives
/// up, keeps the far proxy's code and status and names the machine, so the
/// run ends with `model_loading` rather than `agent_error`.
#[tokio::test]
async fn a_far_refusal_retried_until_the_policy_gives_up_keeps_its_code() {
    let body = r#"{"error":{"message":"Model is loading","type":"service_unavailable","code":"model_loading"}}"#;
    let server = TestServer::start(vec![json(503, "Service Unavailable", body)]).await;
    let one_attempt = RetryPolicy {
        max_attempts: 1,
        ..RetryPolicy::default()
    };

    let error = send_with_retry(
        &Client::new(),
        &format!("{}/v1/chat/completions", server.base_url),
        Some(&paired_machine()),
        &serde_json::json!({"model": "test"}),
        Duration::from_secs(5),
        &one_attempt,
        None,
    )
    .await
    .expect_err("a model still loading after the last attempt is an error");

    let refusal = error
        .downcast_ref::<UpstreamRefusal>()
        .expect("a refusal with a code is typed, retried or not");
    assert_eq!(
        (refusal.status, refusal.code.as_str()),
        (503, "model_loading")
    );
    let message = error.to_string();
    assert!(
        message.starts_with(&format!("{NAME} answered 503 Service Unavailable")),
        "{message}"
    );
    assert!(message.contains("(gave up after 1 attempts"), "{message}");
}
