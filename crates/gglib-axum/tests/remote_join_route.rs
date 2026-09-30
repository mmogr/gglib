//! The route that dials another machine, on the router a shipped daemon
//! builds, `create_embedded_spa_router`.
//!
//! `join` is the only name the daemon answers to for it. The route contract
//! in `daemon_route_contract.rs` shows the new path is routed; these show a
//! request to it reaches the handler, and that the old path reaches nothing.

mod common;

use axum::http::{Method, StatusCode};

use common::origin::{HOST, JSON, send, shipped, shipped_cors};
use gglib_axum::DaemonAccess;
use gglib_core::contracts::http::daemon::REMOTE_JOIN_PATH;

/// An empty join, which asks for the stored pairing, on a machine that has
/// never paired is refused by `RemoteOps::join` itself, with the command
/// that makes a pairing string on the other machine.
#[tokio::test]
async fn an_empty_join_on_a_machine_never_paired_names_gglib_remote_invite() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;

    let answer = send(&app, Method::POST, REMOTE_JOIN_PATH, HOST, &[JSON]).await;

    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.body);
    assert!(
        answer.body.contains("`gglib remote invite`"),
        "the refusal must say where a pairing string comes from: {}",
        answer.body
    );
}

/// No alias keeps the old path. A client built before the rename posts
/// there and gets the shipped router's answer to a `POST` nothing routes,
/// `405` from its `get()` fallback.
#[tokio::test]
async fn the_old_connect_route_answers_405() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;

    let answer = send(&app, Method::POST, "/api/remote/connect", HOST, &[JSON]).await;

    assert_eq!(
        answer.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "{}",
        answer.body
    );
}
