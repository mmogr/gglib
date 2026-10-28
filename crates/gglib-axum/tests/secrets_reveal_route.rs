//! `POST /api/secrets/reveal`: the one route that hands a stored secret back
//! (ADR 0027).
//!
//! That it asks for the daemon's token, and refuses a page on another site,
//! is walked with every other route in `daemon_token_door.rs` and
//! `daemon_origin_routes.rs`.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;
use gglib_core::{CorsConfig, NewMcpServer, RemotePairing, SettingsUpdate};

const PATH: &str = "/api/secrets/reveal";
const PROXY_KEY: &str = "proxy-key-81d0e4";
const DEVICE_KEY: &str = "device-key-5f3a9c";

/// A daemon whose settings hold a proxy key and a pairing, and whose one MCP
/// server holds `TOKEN`; and that server's id.
async fn seeded() -> (axum::Router, i64) {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some(PROXY_KEY.to_owned())),
            remote_pairing: Some(Some(RemotePairing {
                ticket: "pipeabc".to_owned(),
                api_key: DEVICE_KEY.to_owned(),
                default_model: None,
                port: Some(8180),
                name: None,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("seed settings");
    let server =
        NewMcpServer::new_stdio("files", "echo", Vec::new(), None).with_env("TOKEN", "t0k");
    let id = state
        .mcp
        .add_server(server)
        .await
        .expect("seed a server")
        .id;
    (app, id)
}

async fn reveal(app: &axum::Router, body: Value) -> (StatusCode, Value) {
    call_json(app, Method::POST, PATH, Some(body)).await
}

#[tokio::test]
async fn the_proxy_key_is_revealed() {
    let (app, _) = seeded().await;

    let (status, body) = reveal(
        &app,
        json!({ "kind": "setting", "secret": "proxy_api_key" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "value": PROXY_KEY }));
}

#[tokio::test]
async fn an_mcp_env_value_is_revealed_and_an_unknown_one_is_null() {
    let (app, id) = seeded().await;

    let (status, body) = reveal(
        &app,
        json!({ "kind": "mcp_env", "server_id": id, "key": "TOKEN" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "value": "t0k" }));

    let (status, body) = reveal(
        &app,
        json!({ "kind": "mcp_env", "server_id": id, "key": "NOPE" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "value": null }));

    let unknown = id + 1000;
    let (status, body) = reveal(
        &app,
        json!({ "kind": "mcp_env", "server_id": unknown, "key": "TOKEN" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// This device's credential for another machine is not this route's to
/// hand out, to a page or to a client on the network holding the proxy key.
#[tokio::test]
async fn the_pairing_key_is_refused_and_not_sent() {
    let (app, _) = seeded().await;

    let (status, body) = reveal(
        &app,
        json!({ "kind": "setting", "secret": "remote_pairing_key" }),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let text = body.to_string();
    assert!(
        !text.contains(DEVICE_KEY),
        "the key must not be sent: {text}"
    );
    assert!(
        text.contains("gglib config settings reveal remote-pairing.api-key"),
        "the refusal names the command that prints it: {text}"
    );
}

#[tokio::test]
async fn a_body_that_names_no_secret_is_refused() {
    let (app, _) = seeded().await;

    let (status, _) = reveal(&app, json!({ "kind": "setting", "secret": "hf_token" })).await;

    assert!(status.is_client_error(), "{status}");
}
