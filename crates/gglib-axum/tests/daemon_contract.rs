//! Contract tests for the daemon-facing routes: the runtime pin over HTTP
//! and the shutdown route's not-a-daemon refusal.
//!
//! These exercise the exact request shapes `gglib serve`/`gglib proxy
//! stop`/`gglib daemon stop` send. The proxy binds port 0 so no fixed port
//! is contended.

mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::{test_app, test_state_and_app};
use common::origin::authed;
use gglib_core::CorsConfig;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Add a model named `name` to the context's catalog, returning its id.
async fn catalogued(state: &gglib_axum::AxumContext, name: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// `POST /api/proxy/start` on an ephemeral port, pinned to model `id` as
/// `name`.
async fn start_pinned(app: &axum::Router, id: i64, name: &str) -> axum::response::Response {
    let body = format!(r#"{{"port":0,"pinned":{{"id":{id},"name":"{name}"}}}}"#);
    app.clone()
        .oneshot(
            authed()
                .method("POST")
                .uri("/api/proxy/start")
                .header("Host", "127.0.0.1:9887")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// A pinned start applied over HTTP must be reflected in the status, must
/// make the shared runtime refuse foreign models, and must be cleared by a
/// stop — the full `gglib serve` round trip minus the terminal.
#[tokio::test]
async fn pinned_start_pins_the_runtime_and_stop_clears_it() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let pinned = catalogued(&state, "pinned-model").await;
    catalogued(&state, "some-other-model").await;

    // Start pinned, on an ephemeral port so nothing on the machine is hit.
    let response = start_pinned(&app, pinned, "pinned-model").await;
    assert_eq!(response.status(), StatusCode::OK);
    let status = body_json(response).await;
    assert_eq!(
        status.get("pinned_model").and_then(|v| v.as_str()),
        Some("pinned-model"),
        "status must report the pin: {status}"
    );

    // The shared runtime now refuses a foreign model once it has resolved it,
    // and says a model it does not hold is not found, pinned or not.
    let admit = |model: &'static str| {
        state.runtime.admit(
            model,
            None,
            Some(4096),
            gglib_core::ports::LaunchOverrides::default(),
        )
    };
    let err = admit("no-such-model").await.expect_err("nobody has it");
    assert!(
        matches!(err, gglib_core::ports::ModelRuntimeError::ModelNotFound(_)),
        "expected ModelNotFound, got {err:?}"
    );
    let err = admit("some-other-model")
        .await
        .expect_err("a foreign model must be refused while pinned");
    assert!(
        matches!(
            err,
            gglib_core::ports::ModelRuntimeError::PinnedModelMismatch { .. }
        ),
        "expected PinnedModelMismatch, got {err:?}"
    );

    // A second start requesting a different pin is a conflict, not a silent
    // unpinned success. The pin is the model's id: another model carrying the
    // same name is a different pin, and the same id under another name is the
    // same one, started again.
    let response = start_pinned(&app, pinned + 1, "pinned-model").await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = start_pinned(&app, pinned, "renamed-model").await;
    assert_eq!(response.status(), StatusCode::OK);

    // Stop clears the pin.
    let response = app
        .clone()
        .oneshot(
            authed()
                .method("POST")
                .uri("/api/proxy/stop")
                .header("Host", "127.0.0.1:9887")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let status = body_json(response).await;
    assert!(
        status
            .get("pinned_model")
            .is_none_or(serde_json::Value::is_null),
        "stop must clear the pin: {status}"
    );
}

/// `POST /api/daemon/shutdown` on a server that is not hosted by
/// `run_daemon` answers 409 — an embedded or test instance has no daemon
/// lifecycle to end.
#[tokio::test]
async fn shutdown_route_refuses_when_not_a_daemon() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(
            authed()
                .method("POST")
                .uri("/api/daemon/shutdown")
                .header("Host", "127.0.0.1:9887")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
