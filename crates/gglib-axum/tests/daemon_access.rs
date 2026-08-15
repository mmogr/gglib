//! Integration tests for the daemon management API's access control:
//! the Host-header allowlist (DNS-rebinding guard) and the optional
//! bearer token, as applied by `create_router`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use common::harness::test_app_with_access;
use gglib_axum::DaemonAccess;
use gglib_core::CorsConfig;

async fn build_app(access: DaemonAccess) -> axum::Router {
    test_app_with_access(CorsConfig::AllowAll, access).await
}

fn get(uri: &str, host: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("Host", host)
        .body(Body::empty())
        .unwrap()
}

/// The rebinding case the guard exists for: the request reached the socket,
/// but the Host names a hostname this daemon never agreed to answer to.
#[tokio::test]
async fn foreign_host_is_rejected_on_every_route() {
    let app = build_app(DaemonAccess::loopback()).await;

    for uri in ["/api/servers", "/health", "/no/such/path"] {
        let response = app
            .clone()
            .oneshot(get(uri, "evil.example.com"))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{uri} must be Host-guarded"
        );
    }
}

/// A request that omits Host entirely has no claim to check.
#[tokio::test]
async fn missing_host_is_rejected() {
    let app = build_app(DaemonAccess::loopback()).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The unchanged default: loopback clients keep working with no token and
/// no configuration.
#[tokio::test]
async fn loopback_stays_open_by_default() {
    let app = build_app(DaemonAccess::loopback()).await;

    for host in ["127.0.0.1:9887", "localhost:9887", "[::1]:9887"] {
        let response = app.clone().oneshot(get("/health", host)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{host} must be allowed");
    }

    let response = app
        .oneshot(get("/api/servers", "127.0.0.1:9887"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "/api must not require a token when none is configured"
    );
}

/// With a token configured, /api/* requires it — and /health does not, so
/// probes and health checks keep working.
#[tokio::test]
async fn configured_token_gates_api_but_not_health() {
    let app = build_app(DaemonAccess::new(
        Some("s3cret".into()),
        "0.0.0.0",
        Vec::new(),
    ))
    .await;

    let response = app
        .clone()
        .oneshot(get("/api/servers", "127.0.0.1:9887"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "no token → 401"
    );

    let mut wrong = get("/api/servers", "127.0.0.1:9887");
    wrong
        .headers_mut()
        .insert("authorization", "Bearer wrong".parse().unwrap());
    let response = app.clone().oneshot(wrong).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "wrong token → 401"
    );

    let mut right = get("/api/servers", "127.0.0.1:9887");
    right
        .headers_mut()
        .insert("authorization", "Bearer s3cret".parse().unwrap());
    let response = app.clone().oneshot(right).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "right token → 200");

    let response = app.oneshot(get("/health", "127.0.0.1:9887")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "/health stays open");
}

/// A LAN-shared daemon must be reachable by raw IP and by its advertised
/// mDNS name, while still refusing foreign hostnames.
#[tokio::test]
async fn share_lan_policy_accepts_ip_literals_and_named_hosts() {
    let app = build_app(DaemonAccess::new(
        None,
        "0.0.0.0",
        vec!["gglib.local".into()],
    ))
    .await;

    for host in ["192.168.1.7:9887", "gglib.local:9887", "127.0.0.1:9887"] {
        let response = app.clone().oneshot(get("/health", host)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{host} must be allowed");
    }

    let response = app
        .oneshot(get("/health", "evil.example.com"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
