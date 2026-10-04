//! Every `/api` route asks for the daemon's token, on loopback too (#1039).
//!
//! Any account on the machine can reach `127.0.0.1:9887`. Through `/api` it
//! could pair a device, register an MCP server whose command then runs as the
//! owner, or rewrite settings, so the door takes the token only the owner's
//! account can read. A daemon started `--share-lan` takes its API key as well.
//! `/health` stays open, so a probe needs nothing.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use common::harness::{test_app_with_access, with_test_token};
use common::origin::{Answer, HOST, bearer_token, send_request};
use gglib_axum::DaemonAccess;
use gglib_core::CorsConfig;
use gglib_core::contracts::http::{attachments, daemon};

const LAN_KEY: &str = "lan-key";

/// Every `/api` path the CLI's contract names, with its verbs, the
/// parameterized ones instantiated, and the routes a caller without the token
/// could do most harm through.
fn api_routes() -> Vec<(Method, String)> {
    let fixed = daemon::CLI_ROUTE_CONTRACT
        .iter()
        .filter(|(_, path)| path.starts_with("/api/"))
        .map(|(methods, path)| (*methods, (*path).to_owned()));
    let more: [(&[&str], String); 9] = [
        (
            daemon::REMOTE_FORGET_METHODS,
            daemon::remote_forget_path("dev-0a1b2c3d"),
        ),
        (
            daemon::BENCHMARK_TUNE_APPLY_METHODS,
            daemon::benchmark_tune_apply_path(1),
        ),
        (daemon::RUN_METHODS, daemon::run_path("run-1")),
        (daemon::RUN_CANCEL_METHODS, daemon::run_cancel_path("run-1")),
        (&["POST"], "/api/mcp/servers".to_owned()),
        (&["PUT"], "/api/config/settings".to_owned()),
        (&["GET"], "/api/conversations".to_owned()),
        (&["POST"], attachments::ATTACHMENTS_PATH.to_owned()),
        (&["GET"], attachments::attachment_path(&"0".repeat(64))),
    ];
    fixed
        .chain(more)
        .flat_map(|(methods, path)| {
            methods
                .iter()
                .map(move |m| (Method::from_bytes(m.as_bytes()).unwrap(), path.clone()))
        })
        .collect()
}

async fn loopback() -> Router {
    test_app_with_access(
        CorsConfig::AllowAll,
        with_test_token(DaemonAccess::loopback()),
    )
    .await
}

/// A daemon started `--share-lan`: its API key, beside the token.
async fn shared() -> Router {
    let access = DaemonAccess::new(Some(LAN_KEY.into()), "0.0.0.0", Vec::new());
    test_app_with_access(CorsConfig::AllowAll, with_test_token(access)).await
}

/// Send `method path` with `Authorization: <auth>` when there is one.
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

/// The reverse of the route contract: every path it names is refused on
/// loopback without the token, or with another one, and `/health` is not.
#[tokio::test]
async fn without_the_token_every_api_route_is_refused_on_loopback() {
    let app = loopback().await;
    let routes = api_routes();
    assert!(routes.len() >= 25, "the walk found only {routes:?}");
    let mut let_through = Vec::new();
    for (method, path) in routes {
        for auth in [None, Some("Bearer not-the-token")] {
            let answer = call(&app, method.clone(), &path, auth).await;
            if !asked_for_the_token(&answer) {
                let_through.push(format!("  {method} {path} {auth:?}: {}", answer.status));
            }
        }
    }
    assert!(
        let_through.is_empty(),
        "reached without the token:\n{}",
        let_through.join("\n")
    );

    let answer = call(&app, Method::GET, daemon::HEALTH_PATH, None).await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "/health stays open: {}",
        answer.body
    );
}

/// The token gets through, and the handler answers: a body that is not JSON,
/// no tunnel to invite over, no device to forget.
#[tokio::test]
async fn with_the_token_a_request_reaches_its_handler() {
    let token = bearer_token();
    for app in [loopback().await, shared().await] {
        for (method, path, expected) in [
            (
                Method::GET,
                daemon::REMOTE_STATUS_PATH.to_owned(),
                StatusCode::OK,
            ),
            (Method::GET, "/api/conversations".to_owned(), StatusCode::OK),
            (
                Method::POST,
                daemon::REMOTE_ENABLE_PATH.to_owned(),
                StatusCode::BAD_REQUEST,
            ),
            (
                Method::POST,
                daemon::REMOTE_INVITE_PATH.to_owned(),
                StatusCode::CONFLICT,
            ),
            (
                Method::DELETE,
                daemon::remote_forget_path("dev-0a1b2c3d"),
                StatusCode::OK,
            ),
        ] {
            let answer = call(&app, method.clone(), &path, Some(&token)).await;
            assert_eq!(answer.status, expected, "{method} {path}: {}", answer.body);
        }
    }
}

/// A shared daemon takes its key or the token, and asks a caller with
/// neither for the key, which a person may be asked for.
#[tokio::test]
async fn a_shared_daemon_takes_its_key_or_the_token() {
    let app = shared().await;
    let path = daemon::REMOTE_STATUS_PATH;

    for auth in [None, Some("Bearer wrong")] {
        let answer = call(&app, Method::GET, path, auth).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{}", answer.body);
        assert!(answer.body.contains("INVALID_API_KEY"), "{}", answer.body);
    }
    for auth in [format!("Bearer {LAN_KEY}"), bearer_token()] {
        let answer = call(&app, Method::GET, path, Some(&auth)).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    }
}

/// A daemon that could not make its token serves nothing on `/api`, the
/// token it would have held included, and still answers `/health`.
#[tokio::test]
async fn a_daemon_without_a_token_serves_nothing_on_api() {
    let app = test_app_with_access(CorsConfig::AllowAll, DaemonAccess::loopback()).await;
    let token = bearer_token();
    for auth in [None, Some(token.as_str())] {
        let answer = call(&app, Method::GET, daemon::REMOTE_STATUS_PATH, auth).await;
        assert_eq!(
            answer.status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{}",
            answer.body
        );
        assert!(
            answer.body.contains("DAEMON_TOKEN_MISSING"),
            "{}",
            answer.body
        );
    }
    let answer = call(&app, Method::GET, daemon::HEALTH_PATH, None).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
}
