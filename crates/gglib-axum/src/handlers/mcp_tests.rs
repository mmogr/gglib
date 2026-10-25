//! What the add and update routes answer for an SSE server: a bad request,
//! in the service's words.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::access::DaemonToken;
use gglib_core::domain::mcp::NewMcpServer;
use gglib_core::ports::McpServerRepository;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::DaemonAccess;
use crate::handlers::agent::run_fixture::state;
use crate::state::AppState;

/// A router over `state` that asks `/api` for a token of its own, and the
/// `Authorization` header a client holding that token sends.
fn router(state: &AppState) -> (Router, String) {
    let token = DaemonToken::mint().expect("mint a token");
    let bearer = format!("Bearer {}", token.as_str());
    let access = Arc::new(DaemonAccess::loopback().with_daemon_token(Some(token)));
    let app = crate::routes::create_router(state.clone(), &CorsConfig::AllowAll, access);
    (app, bearer)
}

/// Send `body` as JSON to the daemon's router: the status and the answer.
async fn send(
    (app, bearer): &(Router, String),
    method: &str,
    uri: &str,
    body: &Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Host", "127.0.0.1:9887")
        .header("authorization", bearer)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// The update is of a row the repository stored, as a database written while
/// SSE servers were accepted holds one: the add route stores none.
#[tokio::test]
async fn an_sse_server_is_a_bad_request_on_add_and_on_update_in_the_services_words() {
    let (dir, state) = state().await;
    let app = router(&state);
    let refused = (
        StatusCode::BAD_REQUEST,
        json!({
            "error": "SSE servers are not supported yet; only stdio servers can be run",
            "status": 400
        }),
    );

    let new = json!({ "name": "remote", "server_type": "sse", "url": "http://localhost:3001/sse" });
    assert_eq!(send(&app, "POST", "/api/mcp/servers", &new).await, refused);
    assert!(state.mcp_ops.list().await.unwrap().is_empty());

    let url = format!("sqlite:{}", dir.path().join("gglib.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let stored = gglib_db::SqliteMcpRepository::new(pool)
        .insert(NewMcpServer::new_sse("remote", "http://localhost:3001/sse"))
        .await
        .unwrap();
    let route = format!("/api/mcp/servers/{}", stored.id);
    let edit = json!({ "enabled": false });
    assert_eq!(send(&app, "PUT", &route, &edit).await, refused);
    assert!(state.mcp_ops.list().await.unwrap()[0].server.enabled);
}

/// The flag's route: the server as it now stands, 404 for an unknown id, and
/// a bad request for opening an SSE server, in the service's words.
#[tokio::test]
async fn the_remote_route_sets_the_flag_and_refuses_what_it_cannot_open() {
    let (dir, state) = state().await;
    let app = router(&state);
    let new = json!({ "name": "tavily", "server_type": "stdio", "command": "npx" });
    let (status, added) = send(&app, "POST", "/api/mcp/servers", &new).await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let id = added["server"]["id"].as_i64().unwrap();
    assert_eq!(added["server"]["allow_remote"], json!(false));

    let route = format!("/api/mcp/servers/{id}/remote");
    let (status, opened) = send(&app, "PUT", &route, &json!({ "allow": true })).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(
        (&opened["name"], &opened["allow_remote"]),
        (&json!("tavily"), &json!(true))
    );
    assert!(state.mcp_ops.list().await.unwrap()[0].server.allow_remote);

    let unknown = format!("/api/mcp/servers/{}/remote", id + 100);
    let (status, _) = send(&app, "PUT", &unknown, &json!({ "allow": true })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let url = format!("sqlite:{}", dir.path().join("gglib.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let repository = gglib_db::SqliteMcpRepository::new(pool);
    let stored = repository
        .insert(NewMcpServer::new_sse("remote", "http://localhost:3001/sse"))
        .await
        .unwrap();
    let route = format!("/api/mcp/servers/{}/remote", stored.id);
    let refused = send(&app, "PUT", &route, &json!({ "allow": true })).await;
    assert_eq!(
        refused,
        (
            StatusCode::BAD_REQUEST,
            json!({
                "error": "SSE servers are not supported yet; only stdio servers can be run",
                "status": 400
            })
        )
    );

    // An older database can hold a row named `builtin`; `mcp add` refuses one now.
    let builtin = repository
        .insert(NewMcpServer::new_stdio("builtin", "npx", vec![], None))
        .await
        .unwrap();
    let route = format!("/api/mcp/servers/{}/remote", builtin.id);
    let refused = send(&app, "PUT", &route, &json!({ "allow": true })).await;
    assert_eq!(
        refused,
        (
            StatusCode::BAD_REQUEST,
            json!({
                "error": "'builtin' is gglib's own tools and is never opened to devices",
                "status": 400
            })
        )
    );
}
