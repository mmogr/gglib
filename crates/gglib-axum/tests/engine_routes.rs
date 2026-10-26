//! Either engine's routes over the router: the status reports what is
//! installed under this binary's own root and what an install would do, and
//! the removal takes what was installed and the folder it empties. No
//! release is fetched; the install stream itself is pinned beside its
//! handler.

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

/// One test per engine, in order, because the halves read and change that
/// engine's one folder under this binary's root. The status is the engine's
/// own, named by the path: its kind, its pin and its commands. `server`
/// gives the engine's binary path, and is asked only after the root is this
/// binary's own: asked before, it names the real data folder's.
async fn status_then_removal(
    kind: &str,
    server: fn() -> std::path::PathBuf,
    pin: &str,
    word: &str,
) {
    let root = gglib_core::paths::isolate_data_root();
    let server = server();
    assert!(
        server.starts_with(root),
        "{} is outside {}",
        server.display(),
        root.display()
    );
    let base = format!("/api/config/system/engines/{kind}");
    let (status, json) = call(Method::GET, &base).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["kind"], kind, "{json}");
    assert_eq!(json["installed"], false, "{json}");
    assert_eq!(json["pin"]["targetRelease"], pin, "{json}");
    assert_eq!(
        json["installCommand"],
        format!("gglib config {word} install")
    );
    assert_eq!(json["updateCommand"], format!("gglib config {word} update"));
    assert_eq!(json["serving"], serde_json::Value::Null);
    assert!(
        json["download"].is_string() ^ json["downloadUnavailable"].is_string(),
        "either a build to download or the reason there is none: {json}"
    );

    // An install, as far as the status and the removal can tell.
    std::fs::create_dir_all(server.parent().unwrap()).unwrap();
    std::fs::write(&server, b"#!/bin/sh\n").unwrap();
    let (_, json) = call(Method::GET, &base).await;
    assert_eq!(json["installed"], true, "{json}");

    let (status, json) = call(Method::POST, &format!("{base}/uninstall")).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["wasInstalled"], true, "{json}");
    let folder = server.parent().unwrap().parent().unwrap();
    assert!(
        !folder.exists(),
        "{} was emptied and removed",
        folder.display()
    );

    let (status, json) = call(Method::POST, &format!("{base}/uninstall")).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["wasInstalled"], false, "nothing left: {json}");
}

#[tokio::test]
async fn llama_s_status_reads_its_install_and_its_removal_takes_it() {
    status_then_removal(
        "llama",
        || gglib_core::paths::llama_server_path().unwrap(),
        "b11429",
        "llama",
    )
    .await;
}

#[tokio::test]
async fn sd_s_status_reads_its_install_and_its_removal_takes_it() {
    status_then_removal(
        "stable_diffusion",
        || gglib_core::paths::sd_server_path().unwrap(),
        gglib_runtime::sd::PINNED_SD_RELEASE,
        "sd",
    )
    .await;
}

/// The install changes the machine, so it is a POST; a GET does nothing.
/// An engine the path does not name is no route's.
#[tokio::test]
async fn the_install_is_a_post_and_the_kind_is_one_of_two() {
    for kind in ["llama", "stable_diffusion"] {
        let path = format!("/api/config/system/engines/{kind}/install");
        let (status, _) = call(Method::GET, &path).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{path}");
    }
    let (status, _) = call(Method::GET, "/api/config/system/engines/whisper").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
