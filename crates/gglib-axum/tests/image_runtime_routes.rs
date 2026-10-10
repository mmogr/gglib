//! The image runtime's routes over the router: the status reports what is
//! installed under this binary's own root and what an install would do, and
//! the removal takes `.sd/` away whole. No release is fetched; the install
//! stream itself is pinned beside its handler.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;
use gglib_core::CorsConfig;

/// `method path` with the daemon's token, and the JSON it answered.
async fn call(method: Method, path: &str) -> (StatusCode, serde_json::Value) {
    let response = test_app(CorsConfig::AllowAll)
        .await
        .oneshot(
            authed()
                .method(method)
                .header("Host", "127.0.0.1:9887")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

/// One test, in order, because both halves read and change the one `.sd/`
/// under this binary's root.
#[tokio::test]
async fn the_status_reads_the_install_and_the_removal_takes_sd_away_whole() {
    let (status, json) = call(Method::GET, "/api/config/system/sd-status").await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["install"]["installed"], false, "{json}");
    assert_eq!(json["install"]["pinnedRelease"], "master-948-228c707");
    assert_eq!(json["installCommand"], "gglib config sd install");
    assert_eq!(json["runningModel"], serde_json::Value::Null);
    assert!(
        json["prebuilt"].is_string() ^ json["prebuiltUnavailable"].is_string(),
        "either a build to download or the reason there is none: {json}"
    );

    // An install, as far as the status and the removal can tell.
    let server = gglib_core::paths::sd_server_path().unwrap();
    std::fs::create_dir_all(server.parent().unwrap()).unwrap();
    std::fs::write(&server, b"#!/bin/sh\n").unwrap();
    let (_, json) = call(Method::GET, "/api/config/system/sd-status").await;
    assert_eq!(json["install"]["installed"], true, "{json}");

    let (status, json) = call(Method::POST, "/api/config/system/uninstall-sd").await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["wasInstalled"], true, "{json}");
    assert!(!gglib_core::paths::sd_data_dir().unwrap().exists());

    let (status, json) = call(Method::POST, "/api/config/system/uninstall-sd").await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["wasInstalled"], false, "nothing left: {json}");
}

/// The install changes the machine, so it is a POST; a GET does nothing.
#[tokio::test]
async fn the_install_is_a_post() {
    let (status, _) = call(Method::GET, "/api/config/system/install-sd").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}
