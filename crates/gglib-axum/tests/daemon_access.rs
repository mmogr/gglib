//! Integration tests for the daemon management API's access control:
//! the Host-header allowlist (DNS-rebinding guard) and the daemon token, as
//! applied by `create_router`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use common::harness::{test_app_with_access, test_state_and_app, test_token, with_test_token};
use common::origin::bearer_token;
use gglib_axum::DaemonAccess;
use gglib_core::{CorsConfig, SettingsUpdate};

async fn build_app(access: DaemonAccess) -> axum::Router {
    test_app_with_access(CorsConfig::AllowAll, with_test_token(access)).await
}

/// [`get`], carrying `Authorization: <value>`.
fn get_with(uri: &str, value: &str) -> Request<Body> {
    let mut request = get(uri, "127.0.0.1:9887");
    request
        .headers_mut()
        .insert("authorization", value.parse().unwrap());
    request
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

/// Every loopback name reaches `/health`; `/api` takes the daemon token and
/// nothing less.
#[tokio::test]
async fn loopback_asks_the_token() {
    let app = build_app(DaemonAccess::loopback()).await;

    for host in ["127.0.0.1:9887", "localhost:9887", "[::1]:9887"] {
        let response = app.clone().oneshot(get("/health", host)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{host} must be allowed");
    }

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

    let response = app
        .oneshot(get_with("/api/servers", &bearer_token()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "the token → 200");
}

/// The proxy's key neither closes nor opens the daemon.
///
/// `gglib remote enable` mints and persists `proxy_api_key` so the *proxy* can
/// enforce it at the tunnel edge. The management API reads no such thing: it
/// takes the token alone. A router that once re-read `proxy_api_key` on every
/// request 401'd the CLI and the desktop app out of their own daemon when
/// remote access was enabled, including out of `gglib remote disable`.
#[tokio::test]
async fn a_stored_proxy_key_does_not_close_the_loopback_api() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;

    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some("minted-by-remote-enable".into())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("store a proxy api key");

    let response = app
        .clone()
        .oneshot(get_with("/api/servers", &bearer_token()))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a stored proxy_api_key must not close the management API — that is \
         the `gglib remote enable` lockout"
    );
    let response = app
        .oneshot(get_with("/api/servers", "Bearer minted-by-remote-enable"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// The daemon's door and the proxy's must agree about what a valid credential
/// looks like. RFC 9110 makes the scheme case-insensitive, so a client that
/// spells it `bearer` is authenticating correctly.
#[tokio::test]
async fn the_daemon_matches_the_scheme_case_insensitively() {
    let app = build_app(DaemonAccess::loopback()).await;
    let token = test_token();
    let token = token.as_str();

    for header in [
        format!("bearer {token}"),
        format!("BEARER {token}"),
        format!("Bearer  {token}"),
    ] {
        let request = get_with("/api/servers", &header);
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{header:?} presents the token"
        );
    }

    // And the widening stops at the scheme: a different one, and a credential
    // in the wrong case, are still refused.
    let shouted = token.to_uppercase();
    assert_ne!(
        shouted, token,
        "the token has letters to change the case of"
    );
    for header in [
        format!("Basic {token}"),
        format!("Bearer {shouted}"),
        "Bearer ".to_owned(),
        "Bearer".to_owned(),
    ] {
        let request = get_with("/api/servers", &header);
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{header:?} must not authenticate"
        );
    }
}

/// The daemon answers to a host it was named, and not to an IP literal it
/// was not: it listens on loopback, so nothing reaches it by a LAN address.
#[tokio::test]
async fn a_named_host_is_accepted_and_an_unlisted_ip_literal_is_refused() {
    let app = build_app(DaemonAccess::new(vec!["gglib.test".into()])).await;

    for host in ["gglib.test:9887", "127.0.0.1:9887"] {
        let response = app.clone().oneshot(get("/health", host)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{host} must be allowed");
    }

    for host in ["192.168.1.7:9887", "evil.example.com"] {
        let response = app.clone().oneshot(get("/health", host)).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{host} is refused"
        );
    }
}
