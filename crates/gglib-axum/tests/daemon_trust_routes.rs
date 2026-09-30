//! The routes that change who is trusted answer only the daemon token (#1039).
//!
//! On loopback the daemon asks no API key, and any account on the machine can
//! reach it. Enabling the tunnel and inviting a device would hand such a
//! caller a key that outlives it, so those routes, and the others that change
//! who this machine trusts or reaches, ask the token only the owner's account
//! can read. The API key does not open them: the settings route returns it,
//! and on a `--share-lan` daemon the LAN holds it.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use common::harness::{test_app_with_access, with_test_token};
use common::origin::{Answer, HOST, bearer_token, send_request};
use gglib_axum::DaemonAccess;
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon;

const LAN_KEY: &str = "lan-key";

/// Every route that changes who is trusted, and what its handler answers a
/// request that reaches it here: a body that is not JSON where one is read,
/// no tunnel to invite over, no device and no far machine to let go of.
fn trust_routes() -> Vec<(Method, String, StatusCode)> {
    let expected = |path: &str| match path {
        daemon::REMOTE_INVITE_PATH => StatusCode::CONFLICT,
        daemon::REMOTE_DISCONNECT_PATH => StatusCode::OK,
        _ => StatusCode::BAD_REQUEST,
    };
    let mut routes: Vec<_> = daemon::TRUST_ROUTES
        .iter()
        .map(|(method, path)| {
            (
                Method::from_bytes(method.as_bytes()).unwrap(),
                (*path).to_owned(),
                expected(path),
            )
        })
        .collect();
    routes.push((
        Method::DELETE,
        daemon::remote_forget_path("dev-0a1b2c3d"),
        StatusCode::OK,
    ));
    routes
}

async fn loopback() -> Router {
    test_app_with_access(
        CorsConfig::AllowAll,
        with_test_token(DaemonAccess::loopback()),
    )
    .await
}

/// A daemon started `--share-lan`: an API key on every route.
async fn shared() -> Router {
    let access = DaemonAccess::new(Some(LAN_KEY.into()), "0.0.0.0", Vec::new());
    test_app_with_access(CorsConfig::AllowAll, with_test_token(access)).await
}

/// Send `method path`, with `Authorization: <auth>` when there is one, and a
/// JSON content type over a body that is not JSON.
async fn call(app: &Router, method: Method, path: &str, auth: Option<&str>) -> Answer {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", "application/json");
    if let Some(auth) = auth {
        request = request.header("authorization", auth);
    }
    send_request(app, request.body(Body::from("not json")).unwrap()).await
}

fn asked_for_the_token(answer: &Answer) -> bool {
    answer.status == StatusCode::UNAUTHORIZED
        && answer.body.contains(daemon::DAEMON_TOKEN_REQUIRED_TYPE)
        && answer
            .body
            .contains("open the page from the link `gglib web` prints")
}

#[tokio::test]
async fn without_the_token_every_trust_route_is_refused_on_loopback() {
    let app = loopback().await;
    for (method, path, _) in trust_routes() {
        for auth in [None, Some("Bearer not-the-token")] {
            let answer = call(&app, method.clone(), &path, auth).await;
            assert!(
                asked_for_the_token(&answer),
                "{method} {path} {auth:?}: {} {}",
                answer.status,
                answer.body
            );
        }
    }
}

/// The API key opens `/api/*` on a shared daemon, and not these.
#[tokio::test]
async fn the_lan_key_alone_does_not_open_a_trust_route() {
    let app = shared().await;
    let lan = format!("Bearer {LAN_KEY}");
    for (method, path, _) in trust_routes() {
        let answer = call(&app, method.clone(), &path, Some(&lan)).await;
        assert!(
            asked_for_the_token(&answer),
            "{method} {path}: {} {}",
            answer.status,
            answer.body
        );
    }
}

/// The token gets through both guards, the outer one included on a shared
/// daemon, and the handler answers.
#[tokio::test]
async fn with_the_token_every_trust_route_reaches_its_handler() {
    let token = bearer_token();
    for app in [loopback().await, shared().await] {
        for (method, path, expected) in trust_routes() {
            let answer = call(&app, method.clone(), &path, Some(&token)).await;
            assert_eq!(answer.status, expected, "{method} {path}: {}", answer.body);
        }
    }
}

/// Every other route is as it was: open on loopback without a credential.
#[tokio::test]
async fn a_route_that_trusts_nobody_new_is_unchanged_on_loopback() {
    let app = loopback().await;
    for (method, path) in [
        (Method::GET, daemon::REMOTE_STATUS_PATH),
        (Method::GET, daemon::REMOTE_DEVICES_PATH),
        (Method::GET, "/api/conversations"),
    ] {
        let answer = call(&app, method.clone(), path, None).await;
        assert_eq!(
            answer.status,
            StatusCode::OK,
            "{method} {path}: {}",
            answer.body
        );
    }
}

/// A shared daemon still asks its key on ordinary routes, and takes the
/// daemon token there too.
#[tokio::test]
async fn a_shared_daemon_still_needs_its_key_on_ordinary_routes() {
    let app = shared().await;
    let path = daemon::REMOTE_STATUS_PATH;

    let answer = call(&app, Method::GET, path, None).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{}", answer.body);
    assert!(answer.body.contains("INVALID_API_KEY"), "{}", answer.body);

    for auth in [format!("Bearer {LAN_KEY}"), bearer_token()] {
        let answer = call(&app, Method::GET, path, Some(&auth)).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    }
}

/// A daemon that could not read or mint its token shuts the trust routes to
/// everybody rather than opening them.
#[tokio::test]
async fn a_daemon_without_a_token_opens_no_trust_route() {
    let app = test_app_with_access(CorsConfig::AllowAll, DaemonAccess::loopback()).await;
    let token = bearer_token();
    for (method, path, _) in trust_routes() {
        let answer = call(&app, method.clone(), &path, Some(&token)).await;
        assert!(
            asked_for_the_token(&answer),
            "{method} {path}: {}",
            answer.body
        );
    }
}
