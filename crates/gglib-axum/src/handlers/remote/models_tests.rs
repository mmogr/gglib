//! Each model read against a fake far proxy on a loopback port: the path it
//! reaches, the key it carries, what this daemon answers with, and what a far
//! refusal or a far machine on an older build becomes.

use axum::http::StatusCode;

use super::super::fake_far::{FINGERPRINT, carries_key, far, far_untold, json, only, read};
use super::{list_models_via, load_model_via, model_detail_via};

/// A far list as a gglib proxy writes it: a base entry and its profile
/// variant, which shares its `gglib_id`, and who answered.
const LISTED: &str = r#"{"object":"list","machine_name":"desk",
    "gglib":{"version":"9.9.9","features":["chats","runs","drawing","status"]},"data":[
    {"id":"qwen3","gglib_id":3,"object":"model","created":1,"owned_by":"gglib","context_window":30000},
    {"id":"qwen3:coding","gglib_id":3,"profile":"coding","object":"model","created":1,"owned_by":"gglib"},
    {"id":"llama","gglib_id":1000,"object":"model","created":1,"owned_by":"gglib"}]}"#;

/// One model as the far detail route answers it.
const LOOKUP: &str = r#"{"profile":"coding","detail":{"id":3,"name":"org/qwen3",
    "paramCountB":8.0,"addedAt":"2026-10-01 09:00:00","isServing":true,"metadata":{}}}"#;

/// What a far machine on a gglib from before devices said their version is
/// asked to do.
const UPDATE: &str = "the paired machine runs a gglib older than this machine's";

#[tokio::test]
async fn the_list_is_every_far_entry_with_its_machine_and_what_may_be_done_there() {
    let (fake, far) = far(200, LISTED).await;

    let (status, body) = read(list_models_via(&far).await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let body = json(&body);
    assert_eq!(
        body["machine"],
        serde_json::json!({ "kind": "paired", "fingerprint": FINGERPRINT })
    );
    assert_eq!(
        body["actions"],
        serde_json::json!(["list", "detail", "chat", "load"])
    );
    let ids: Vec<_> = body["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["id"].as_str().unwrap(), m["gglib_id"].as_i64().unwrap()))
        .collect();
    assert_eq!(
        ids,
        [("qwen3", 3), ("qwen3:coding", 3), ("llama", 1000)],
        "variants are kept, in the far machine's order"
    );
    assert_eq!(body["models"][1]["profile"], "coding");
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", "/v1/models")
    );
    assert!(carries_key(&seen), "{seen:?}");
}

/// A far gglib from before devices said their version is told to update,
/// not shown a list it may not be asked about, even one from before models
/// carried ids, which this build could not read.
#[tokio::test]
async fn a_far_list_that_does_not_say_its_version_asks_that_machine_to_update() {
    let older = r#"{"object":"list","data":[{"id":"qwen3","object":"model","created":1,"owned_by":"gglib"}]}"#;
    let (_, far) = far(200, older).await;

    let err = list_models_via(&far).await.unwrap_err();

    let (status, body) = read(axum::response::IntoResponse::into_response(err)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        json(&body)["error"].as_str().unwrap().contains(UPDATE),
        "{body}"
    );
}

/// A `401` from here would have the page ask for this daemon's key.
#[tokio::test]
async fn a_refused_key_on_the_list_is_a_conflict_that_says_to_pair_again() {
    let (_, far) = far(401, r#"{"error":{"message":"Invalid API key"}}"#).await;

    let (status, body) = read(list_models_via(&far).await.unwrap()).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json(&body)["type"], "key_refused");
}

/// The identifier travels as one path segment, so a name holding `/` and a
/// `:profile` suffix arrive at the far route whole.
#[tokio::test]
async fn a_model_is_read_by_its_identifier_as_one_segment() {
    let (fake, far) = far(200, LOOKUP).await;

    let (status, body) = read(model_detail_via(&far, "org/qwen3:coding").await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let body = json(&body);
    assert_eq!(body["profile"], "coding");
    assert_eq!(body["detail"]["id"], 3);
    assert_eq!(body["detail"]["isServing"], true);
    let seen = only(&fake);
    assert_eq!(seen.uri, "/v1/gglib/models/org%2Fqwen3%3Acoding/detail");
    assert!(carries_key(&seen), "{seen:?}");
}

/// A model the far machine does not have is its own `404`, code and all.
#[tokio::test]
async fn a_far_404_that_names_its_code_is_relayed() {
    let refusal = r#"{"error":{"message":"Model '9' not found","type":"invalid_request_error","code":"model_not_found"}}"#;
    let (_, far) = far(404, refusal).await;

    let (status, body) = read(model_detail_via(&far, "9").await.unwrap()).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        json(&body),
        serde_json::json!({ "error": "Model '9' not found", "status": 404, "type": "model_not_found" })
    );
}

/// A model asked of a far gglib too old to say its version asks that machine
/// to update, before the detail route is asked.
#[tokio::test]
async fn a_model_asked_of_a_far_machine_too_old_to_say_asks_it_to_update() {
    let (fake, far) = far_untold(200, r#"{"object":"list","data":[]}"#).await;

    let err = model_detail_via(&far, "3").await.unwrap_err();

    let (status, body) = read(axum::response::IntoResponse::into_response(err)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        json(&body)["error"].as_str().unwrap().contains(UPDATE),
        "{body}"
    );
    assert_eq!(only(&fake).uri, "/v1/models");
}

#[tokio::test]
async fn a_load_posts_with_the_key_and_its_context() {
    let loaded = r#"{"model":"org/qwen3","started":true,"context":8192}"#;
    let (fake, far) = far(200, loaded).await;

    let (status, body) = read(load_model_via(&far, "org/qwen3", Some(8192)).await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(json(&body), json(loaded));
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("POST", "/v1/gglib/models/org%2Fqwen3/load")
    );
    assert!(carries_key(&seen), "{seen:?}");
    assert_eq!(json(&seen.body), serde_json::json!({ "num_ctx": 8192 }));
}

/// A far machine still busy says when to come back, and that reaches the
/// caller.
#[tokio::test]
async fn a_busy_far_load_says_when_to_come_back() {
    let busy = r#"{"error":{"message":"queued too long","code":"admission_timeout"}}"#;
    let (fake, far) = far(503, busy).await;
    *fake.retry_after.lock().unwrap() = Some("30");

    let response = load_model_via(&far, "qwen3", None).await.unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .unwrap(),
        "30"
    );
}
