//! `/api/remote/invite`, `/api/remote/devices` and its `{device}` sibling.
//!
//! `daemon_route_contract.rs` proves these paths are *routed*; this proves
//! they reach `RemoteOps` and that what comes back is the shape the surfaces
//! were written against. A new file rather than more of
//! `integration_routes.rs`, which is at the size the complexity ratchet
//! recorded for it and may not grow.
//!
//! None of it needs a tunnel, which is the point. `list` and `forget` work
//! with remote access off — a laptop is lost at a moment nobody chose, and a
//! retirement that needed the tunnel up would be one more thing to do first
//! — and `invite` is the one that cannot, so its refusal is the control.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Every request carries the host guard's header, or it is refused before a
/// handler is reached and the test proves nothing about the route.
fn request(method: &str, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("Host", "127.0.0.1:9887")
        .body(Body::empty())
        .unwrap()
}

/// A machine that has never paired anything answers with an empty list, not
/// a 404 and not an error.
///
/// The GUI renders this on every open of the Remote panel, so "nothing yet"
/// has to be an ordinary answer it can map over.
#[tokio::test]
async fn a_machine_that_has_paired_nothing_lists_no_devices() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(request("GET", daemon::REMOTE_DEVICES_PATH))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body,
        serde_json::json!([]),
        "an empty roster is an empty array, not null and not an object: {body}"
    );
}

/// Retiring a device this machine never issued a key to is a `200` saying so.
///
/// Not a `404`: the outcome asked for is that nothing is held under that
/// name, and nothing is. The flag is there for a surface that wants to tell
/// the difference; one that only wants the device gone can ignore it.
#[tokio::test]
async fn forgetting_a_device_that_was_never_held_says_so_rather_than_failing() {
    let app = test_app(CorsConfig::AllowAll).await;

    let path = daemon::remote_forget_path("dev-0a1b2c3d");
    let response = app.oneshot(request("DELETE", &path)).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a device that is already gone is the outcome asked for"
    );
    let body = body_json(response).await;
    assert_eq!(
        body.get("forgotten").and_then(serde_json::Value::as_bool),
        Some(false),
        "and the body says nothing was held: {body}"
    );
}

/// The control, and the one route that needs the tunnel: inviting with
/// remote access off is a `409`, and the refusal says which of the two
/// commands to run.
///
/// Without this the two tests above would pass just as well against handlers
/// that answered everything, since both of their expected answers are the
/// empty case.
#[tokio::test]
async fn inviting_with_the_tunnel_down_is_refused_and_says_what_to_run() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(request("POST", daemon::REMOTE_INVITE_PATH))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "there is no session to offer a code against"
    );
    let body = body_json(response).await;
    let message = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        message.contains("gglib remote enable"),
        "the refusal names the command that fixes it: {body}"
    );
}

/// The device id travels in the path, so a surface has to be able to send
/// one that is not a bare word without the route losing it.
///
/// Ids are minted `dev-<hex>` and the edge holds a token under nothing else,
/// so this is a guard on the routing rather than on the id: `/devices` and
/// `/devices/{device}` are siblings, and a request for the second must not
/// be answered by the first.
#[tokio::test]
async fn the_device_id_reaches_the_handler_rather_than_matching_the_list() {
    let app = test_app(CorsConfig::AllowAll).await;

    let path = daemon::remote_forget_path("dev-ffffffff");
    let response = app.oneshot(request("DELETE", &path)).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the sibling route answered, not the collection"
    );
    let body = body_json(response).await;
    assert!(
        body.get("forgotten").is_some(),
        "and it answered with the forget shape, not a device list: {body}"
    );
}
