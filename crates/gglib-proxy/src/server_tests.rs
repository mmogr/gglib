use super::*;

#[tokio::test]
async fn test_health_check() {
    let response = health_check().await.into_response();
    assert_eq!(response.status(), StatusCode::OK);
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
