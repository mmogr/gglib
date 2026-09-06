//! Integration tests for the daemon management API's access control:
//! the Host-header allowlist (DNS-rebinding guard) and the optional
//! bearer token, as applied by `create_router`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use common::harness::{test_app_with_access, test_state_and_app, test_state_and_app_with_access};
use gglib_axum::DaemonAccess;
use gglib_core::{CorsConfig, SettingsUpdate};

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

/// The regression this file exists to prevent from returning.
///
/// `gglib remote enable` mints and persists `proxy_api_key` so the *proxy* can
/// enforce it at the tunnel edge. The management API reads no such thing: it
/// bound on loopback with no token, and `DaemonAccess::new`'s contract is that
/// no token means unauthenticated. Before the fix the router rebuilt the policy
/// as `tracking(None, settings)`, which re-read `proxy_api_key` on every
/// request — so enabling remote access 401'd the CLI and the desktop app out of
/// their own daemon, including out of `gglib remote disable`.
///
/// The write happens before the first request on purpose: `SettingsCache` is
/// lazily populated, so its very first `get()` already sees the key. No sleep,
/// and no flake in either direction.
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
        .oneshot(get("/api/servers", "127.0.0.1:9887"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a proxy_api_key set after bind must not close a management API that \
         bound without one — that is the `gglib remote enable` lockout"
    );
}

/// The other half, so the fix above cannot be mistaken for "loopback is always
/// open" or "the daemon ignores settings".
///
/// A daemon that bound *with* a key must keep following the stored value, which
/// is what makes rotation through `gglib config settings set` reach a running
/// listener without a restart. Tracking is right here and wrong above; the
/// difference is whether a token was in force at bind.
#[tokio::test]
async fn a_bound_key_still_follows_a_rotation() {
    let (state, app) = test_state_and_app_with_access(
        CorsConfig::AllowAll,
        DaemonAccess::new(Some("bound-key".into()), "0.0.0.0", Vec::new()),
    )
    .await;

    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some("rotated-key".into())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("rotate the stored key");

    let mut rotated = get("/api/servers", "127.0.0.1:9887");
    rotated
        .headers_mut()
        .insert("authorization", "Bearer rotated-key".parse().unwrap());
    let response = app.clone().oneshot(rotated).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a rotation must reach a listener that bound with a key"
    );

    let mut stale = get("/api/servers", "127.0.0.1:9887");
    stale
        .headers_mut()
        .insert("authorization", "Bearer bound-key".parse().unwrap());
    let response = app.oneshot(stale).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "the superseded key must stop working"
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

/// The daemon's door and the proxy's must agree about what a valid credential
/// looks like. RFC 9110 makes the scheme case-insensitive, so a client that
/// spells it `bearer` is authenticating correctly.
#[tokio::test]
async fn the_daemon_matches_the_scheme_case_insensitively() {
    let app = build_app(DaemonAccess::new(
        Some("s3cret".into()),
        "0.0.0.0",
        Vec::new(),
    ))
    .await;

    for header in ["bearer s3cret", "BEARER s3cret", "Bearer  s3cret"] {
        let mut request = get("/api/servers", "127.0.0.1:9887");
        request
            .headers_mut()
            .insert("authorization", header.parse().unwrap());
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{header:?} presents the configured token"
        );
    }

    // And the widening stops at the scheme: a different one, and a credential
    // in the wrong case, are still refused.
    for header in ["Basic s3cret", "Bearer S3CRET", "Bearer ", "Bearer"] {
        let mut request = get("/api/servers", "127.0.0.1:9887");
        request
            .headers_mut()
            .insert("authorization", header.parse().unwrap());
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{header:?} must not authenticate"
        );
    }
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
