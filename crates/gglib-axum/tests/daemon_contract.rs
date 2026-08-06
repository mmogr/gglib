//! Contract tests for the daemon-facing routes: the runtime pin over HTTP
//! and the shutdown route's not-a-daemon refusal.
//!
//! These exercise the exact request shapes `gglib serve`/`gglib proxy
//! stop`/`gglib daemon stop` send. The proxy binds port 0 so no fixed port
//! is contended.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::ports::TEST_BASE_PORT;
use gglib_axum::DaemonAccess;
use gglib_axum::bootstrap::{ServerConfig, bootstrap};
use gglib_axum::routes::create_router;
use gglib_core::CorsConfig;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent_agent_loops: 1,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// A pinned start applied over HTTP must be reflected in the status, must
/// make the shared runtime refuse foreign models, and must be cleared by a
/// stop — the full `gglib serve` round trip minus the terminal.
#[tokio::test]
async fn pinned_start_pins_the_runtime_and_stop_clears_it() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return, // Skip if the environment has no DB path
    };
    let state = std::sync::Arc::new(ctx);
    let app = create_router(
        std::sync::Arc::clone(&state),
        &CorsConfig::AllowAll,
        std::sync::Arc::new(DaemonAccess::loopback()),
    );

    // Start pinned, on an ephemeral port so nothing on the machine is hit.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/proxy/start")
                .header("Host", "127.0.0.1:9887")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"port":0,"pinned":{"name":"pinned-model"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let status = body_json(response).await;
    assert_eq!(
        status.get("pinned_model").and_then(|v| v.as_str()),
        Some("pinned-model"),
        "status must report the pin: {status}"
    );

    // The shared runtime now refuses foreign models outright.
    let err = state
        .runtime
        .admit(
            "some-other-model",
            None,
            4096,
            gglib_core::ports::LaunchOverrides::default(),
        )
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
    // unpinned success.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/proxy/start")
                .header("Host", "127.0.0.1:9887")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"port":0,"pinned":{"name":"another"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Stop clears the pin.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
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
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(
        std::sync::Arc::new(ctx),
        &CorsConfig::AllowAll,
        std::sync::Arc::new(DaemonAccess::loopback()),
    );

    let response = app
        .oneshot(
            Request::builder()
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
