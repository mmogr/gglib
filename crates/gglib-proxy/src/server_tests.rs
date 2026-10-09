use super::*;

/// `/health` answers before any credential is checked, so it says it is up
/// and nothing else: the machine's name is on `/v1/models`, behind the bearer.
#[tokio::test]
async fn test_health_check() {
    let response = health_check().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn test_admission_timeout_returns_503_with_retry_after() {
    let err = ModelRuntimeError::AdmissionTimeout("test timeout".to_string());
    let response = handle_runtime_error(err);
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        response
            .headers()
            .contains_key(axum::http::header::RETRY_AFTER)
    );
}

/// The advertised hint must come from the shared policy, not a literal, so
/// it cannot drift from the backoff our own clients actually apply.
#[tokio::test]
async fn retry_after_is_a_delay_the_policy_could_produce() {
    let response = handle_runtime_error(ModelRuntimeError::AdmissionTimeout("c".to_string()));
    let policy = RetryPolicy::default();

    let advertised = response
        .headers()
        .get(axum::http::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .expect("Retry-After must be a whole number of seconds");

    assert!(advertised >= 1, "a zero hint invites an immediate hot loop");
    assert!(
        advertised <= policy.max_backoff.as_secs(),
        "advertising longer than the policy's own ceiling asks clients to \
         wait longer than we ever would: {advertised}s"
    );
    assert!(
        std::time::Duration::from_secs(advertised) < policy.total_deadline,
        "a hint at or past the whole budget leaves no room for a retry"
    );
}

/// An oversubscribed queue and ordinary loading are indistinguishable on
/// the wire — both serialise to `service_unavailable` — so the reason
/// header is the only way a dashboard can tell them apart.
#[tokio::test]
async fn only_an_admission_timeout_carries_the_retry_reason_header() {
    let queued_out = handle_runtime_error(ModelRuntimeError::AdmissionTimeout("c".to_string()));
    assert_eq!(
        queued_out
            .headers()
            .get(RETRY_REASON_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(RETRY_REASON_ADMISSION)
    );

    let loading = handle_runtime_error(ModelRuntimeError::ModelLoading);
    assert_eq!(loading.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        loading
            .headers()
            .contains_key(axum::http::header::RETRY_AFTER),
        "loading is still retryable and still advertises a hint"
    );
    assert!(
        !loading.headers().contains_key(RETRY_REASON_HEADER),
        "only an admission timeout is labelled"
    );
}

/// A terminal error must not invite a retry.
#[tokio::test]
async fn non_retryable_errors_carry_neither_header() {
    let response = handle_runtime_error(ModelRuntimeError::ModelNotFound("nope".to_string()));
    assert_ne!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !response
            .headers()
            .contains_key(axum::http::header::RETRY_AFTER)
    );
    assert!(!response.headers().contains_key(RETRY_REASON_HEADER));
}

/// One image refusal as the client receives it: status, the body's type and
/// code and message, and whether it says when to retry.
async fn image_refusal(err: ModelRuntimeError) -> (StatusCode, serde_json::Value, bool) {
    let response = handle_runtime_error(err);
    let status = response.status();
    let retry_after = response
        .headers()
        .contains_key(axum::http::header::RETRY_AFTER);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, serde_json::from_slice(&body).unwrap(), retry_after)
}

/// A missing image runtime is a 503 that names the install command and
/// never says when to retry: retrying cannot install it.
#[tokio::test]
async fn image_runtime_not_installed_is_a_503_without_retry_after() {
    let (status, body, retry_after) =
        image_refusal(ModelRuntimeError::ImageRuntimeNotInstalled).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !retry_after,
        "a missing runtime does not come back by waiting"
    );
    assert_eq!(body["error"]["type"], "server_error");
    assert_eq!(body["error"]["code"], "image_runtime_not_installed");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("gglib config sd install"), "{message}");
}

/// An image model missing a file is a 400 naming every missing role.
#[tokio::test]
async fn image_model_incomplete_is_a_400_naming_every_missing_role() {
    let (status, body, retry_after) = image_refusal(ModelRuntimeError::ImageModelIncomplete {
        model: "flux1-schnell".to_owned(),
        missing: vec![
            gglib_core::domain::ComponentRole::Vae,
            gglib_core::domain::ComponentRole::T5xxl,
        ],
    })
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!retry_after);
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert_eq!(body["error"]["code"], "image_model_incomplete");
    let message = body["error"]["message"].as_str().unwrap();
    for named in [
        "'flux1-schnell'",
        "VAE",
        "T5-XXL",
        "vae=<path>",
        "t5xxl=<path>",
    ] {
        assert!(message.contains(named), "{named} missing from {message}");
    }
}

/// No room beside a held model is the one image refusal worth retrying: a
/// 503 with Retry-After, naming the held model and the bytes.
#[tokio::test]
async fn image_model_does_not_fit_is_a_retryable_503_naming_the_held_model() {
    const GIB: u64 = 1024 * 1024 * 1024;
    let (status, body, retry_after) = image_refusal(ModelRuntimeError::ImageModelDoesNotFit {
        model: "flux1-schnell".to_owned(),
        held_model: "qwen3-27b".to_owned(),
        needed_bytes: Some(28 * GIB),
        free_bytes: Some(9 * GIB),
    })
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(retry_after, "a hold ends; the client may come back");
    assert_eq!(body["error"]["type"], "service_unavailable");
    assert_eq!(body["error"]["code"], "image_model_does_not_fit");
    let message = body["error"]["message"].as_str().unwrap();
    for named in ["'flux1-schnell'", "'qwen3-27b'", "28.00 GiB", "9.00 GiB"] {
        assert!(message.contains(named), "{named} missing from {message}");
    }
}
