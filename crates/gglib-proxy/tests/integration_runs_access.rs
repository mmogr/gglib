//! `/v1/runs/*` sits where the proxy's other routes a credential reaches do:
//! inside the bearer guard and the device gate. A run route added beside
//! `/health` fails here.

use std::sync::Arc;

use gglib_core::ports::RunScope;
use reqwest::StatusCode;

mod fixtures;
use fixtures::runs::{FakeRuns, code, json, routes, serve_demanding};

/// The run routes are in the protected group: with a key set, each is
/// refused without the token and served with it. Modelled on
/// `integration_auth.rs`'s dashboard test; a route added beside `/health`
/// would be served without one.
#[tokio::test]
async fn every_run_route_requires_the_token() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve_demanding(Some("secret123"), Some(Arc::clone(&runs))).await;
    for request in routes(&base) {
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
    assert!(runs.scopes().is_empty(), "nothing reached the runs");
    for request in routes(&base) {
        let status = request
            .bearer_auth("secret123")
            .send()
            .await
            .unwrap()
            .status();
        assert!(status.is_success(), "{status}");
    }
    assert_eq!(runs.scopes(), vec![RunScope::Local; 5]);
    cancel.cancel();
}

/// The device gate, not only the scope extractor, covers every run route:
/// a tunnelled request that names no device and carries the token is refused
/// with the gate's `device_not_paired`. The extractor answers
/// `device_not_named`, so a route that left the group fails here.
#[tokio::test]
async fn the_device_gate_covers_every_run_route() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve_demanding(Some("secret123"), Some(Arc::clone(&runs))).await;
    for request in routes(&base) {
        let response = request
            .bearer_auth("secret123")
            .header("via", "1.1 modelpipe")
            .send()
            .await
            .unwrap();
        let (status, body) = json(response).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_paired", "{body}");
    }
    assert!(runs.scopes().is_empty());
    cancel.cancel();
}
