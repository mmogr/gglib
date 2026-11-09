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
    catalogued_at(state, name, name).await
}

/// Add a model named `name`, its file `/models/<file>.gguf`, returning its id.
async fn catalogued_at(state: &gglib_axum::AxumContext, name: &str, file: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{file}.gguf")),
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
    assert_eq!(
        status
            .get("pinned_model_id")
            .and_then(serde_json::Value::as_i64),
        Some(pinned),
        "status must report the pin's id: {status}"
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
    let status = body_json(response).await;
    assert_eq!(
        status
            .get("pinned_model_id")
            .and_then(serde_json::Value::as_i64),
        Some(pinned),
        "a renamed pin keeps its id: {status}"
    );

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
    assert!(
        status
            .get("pinned_model_id")
            .is_none_or(serde_json::Value::is_null),
        "stop must clear the pin's id: {status}"
    );
}

/// Of two models that share a name, the status read back names the pinned
/// one by its id, so a surface that matches by id counts only that one as
/// pinned.
#[tokio::test]
async fn a_pin_on_one_of_two_same_named_models_names_that_one_by_id() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let first = catalogued_at(&state, "twin", "twin-a").await;
    let second = catalogued_at(&state, "twin", "twin-b").await;
    assert_ne!(first, second);

    let response = start_pinned(&app, second, "twin").await;
    assert_eq!(response.status(), StatusCode::OK);
    let status = status_of(&app).await;
    assert_eq!(status["pinned_model"], "twin", "{status}");
    assert_eq!(status["pinned_model_id"], second, "{status}");

    let stopped = app
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
    assert_eq!(stopped.status(), StatusCode::OK);
}

/// `GET /api/proxy/status`, read whole.
async fn status_of(app: &axum::Router) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            authed()
                .uri("/api/proxy/status")
                .header("Host", "127.0.0.1:9887")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

/// A model renamed while it is pinned is named in the status, and in its
/// Pinned to sentence, by its new name, since the pin is its id; a model
/// gone from the catalogue is named as it was pinned.
#[tokio::test]
async fn a_pinned_model_renamed_is_named_anew_and_one_removed_as_it_was_pinned() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = catalogued(&state, "before-rename").await;
    let response = start_pinned(&app, id, "before-rename").await;
    assert_eq!(response.status(), StatusCode::OK);

    let mut model = state.core.models().get_by_id(id).await.unwrap().unwrap();
    model.name = "after-rename".to_owned();
    state.core.models().update(&model).await.unwrap();
    let status = status_of(&app).await;
    assert_eq!(status["pinned_model"], "after-rename", "{status}");
    assert_eq!(status["pinned_to"], "Pinned to after-rename", "{status}");
    assert_eq!(status["pinned_model_id"], id, "{status}");

    state.core.models().delete(id).await.unwrap();
    let status = status_of(&app).await;
    assert_eq!(status["pinned_model"], "before-rename", "{status}");
    assert_eq!(status["pinned_to"], "Pinned to before-rename", "{status}");
    assert_eq!(status["pinned_model_id"], id, "{status}");

    let stopped = app
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
    assert_eq!(stopped.status(), StatusCode::OK);
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
