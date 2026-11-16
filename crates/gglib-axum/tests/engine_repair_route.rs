//! The engines' repair route makes one repair, the Visual C++ runtime's
//! install, and refuses (409) everything else: an engine that is not
//! there, one that works, and one whose fix is a reinstall. Off Windows no
//! engine is ever offered the runtime, so on macOS and Linux every call is
//! refused and nothing is downloaded or run. A test binary of its own: it
//! changes llama.cpp's folder under this binary's root.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;
use gglib_core::CorsConfig;

const REPAIR: &str = "/api/config/system/engines/llama/repair";

/// `POST` the repair with the daemon's token, and its status and body.
async fn repair() -> (StatusCode, String) {
    let response = test_app(CorsConfig::AllowAll)
        .await
        .oneshot(
            authed()
                .method(Method::POST)
                .header("Host", "127.0.0.1:9887")
                .uri(REPAIR)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn the_repair_route_refuses_all_but_the_runtime_s_install() {
    let root = gglib_core::paths::isolate_data_root();
    let server = gglib_core::paths::llama_server_path().unwrap();
    assert!(server.starts_with(root), "{}", server.display());

    // Not installed: no fix.
    let (status, body) = repair().await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("nothing gglib can repair"), "{body}");

    // A file that is no program: its fix, where it has one, is a reinstall.
    let bin = server.parent().unwrap();
    std::fs::create_dir_all(bin).unwrap();
    std::fs::write(&server, b"not a program").unwrap();
    let (status, body) = repair().await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    std::fs::remove_file(&server).unwrap();

    // An engine that works: nothing to repair.
    let stem = server.file_stem().unwrap().to_string_lossy().into_owned();
    let program = gglib_test_proc::install(bin, &stem, "print version: 1 (abc)\nexit 0");
    assert_eq!(program, server);
    let (status, body) = repair().await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("nothing gglib can repair"), "{body}");
}

/// The repair changes the machine, so it is a POST; a GET does nothing.
#[tokio::test]
async fn the_repair_is_a_post() {
    let response = test_app(CorsConfig::AllowAll)
        .await
        .oneshot(
            authed()
                .header("Host", "127.0.0.1:9887")
                .uri(REPAIR)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
