//! A page on another site cannot change anything through the daemon (#1118).
//!
//! A loopback daemon asks no credential, and a browser opens its socket for
//! any page it has loaded. CORS hides the answer from that page; it does not
//! stop the request, and a form post or a `no-cors` fetch goes without a
//! preflight. The origin guard is what refuses it. These tests build the
//! router a shipped daemon builds, `create_embedded_spa_router`, under the
//! CORS config `gglib daemon run` and the desktop app start it with.
//! `daemon_origin_routes.rs` walks the routes.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode};

use common::harness::test_state;
use common::origin::{
    Answer, ELSEWHERE, FORM, HOST, JSON, send, send_request, shipped, shipped_cors,
};
use gglib_axum::{CorsConfig, DaemonAccess};
use gglib_core::contracts::http::daemon;

async fn disconnect(app: &Router, host: &str, headers: &[(&str, &str)]) -> Answer {
    let path = daemon::REMOTE_DISCONNECT_PATH;
    send(app, Method::POST, path, host, headers).await
}

fn refused(answer: &Answer) -> bool {
    answer.refused_for_its_origin()
}

#[tokio::test]
async fn a_cross_site_post_to_remote_disconnect_is_refused() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let answer = disconnect(&app, HOST, &[("origin", ELSEWHERE), FORM]).await;
    assert!(refused(&answer), "{} {}", answer.status, answer.body);
}

/// Under the config that lets every page read, so the refusal is the guard's
/// own rule for `null` rather than an origin missing from a list.
#[tokio::test]
async fn a_page_that_hides_its_origin_is_refused() {
    let app = shipped(&CorsConfig::AllowAll, DaemonAccess::loopback()).await;
    let answer = disconnect(&app, HOST, &[("origin", "null"), FORM]).await;
    assert!(refused(&answer), "{} {}", answer.status, answer.body);
}

/// No browser sends an `Origin` that is not text, but one that arrives is
/// refused. Read as absent, it would pass as a program's request.
#[tokio::test]
async fn an_origin_that_is_not_text_is_refused_not_read_as_absent() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let origin = HeaderValue::from_bytes(b"http://\xffevil.example").unwrap();
    let request = Request::post(daemon::REMOTE_DISCONNECT_PATH)
        .header("host", HOST)
        .header("origin", origin)
        .header(FORM.0, FORM.1)
        .body(Body::empty())
        .unwrap();
    let answer = send_request(&app, request).await;
    assert!(refused(&answer), "{} {}", answer.status, answer.body);
}

#[tokio::test]
async fn a_cross_site_request_without_an_origin_is_refused_by_its_fetch_metadata() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let answer = disconnect(&app, HOST, &[("sec-fetch-site", "cross-site"), FORM]).await;
    assert!(refused(&answer), "{} {}", answer.status, answer.body);
}

/// Every origin the shipped config lets read: the desktop app on each
/// platform and the Vite dev server. Each is cross-site by fetch metadata,
/// which must not count against it.
#[tokio::test]
async fn the_desktop_app_and_the_dev_server_still_change_things() {
    let cors = shipped_cors();
    let CorsConfig::AllowOrigins(origins) = &cors else {
        panic!("the daemon ships with an origin list, not {cors:?}");
    };
    for expected in [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
        "http://localhost:5173",
    ] {
        assert!(origins.iter().any(|o| o == expected), "{origins:?}");
    }
    let app = shipped(&cors, DaemonAccess::loopback()).await;
    for origin in origins {
        let headers = [
            ("origin", origin.as_str()),
            ("sec-fetch-site", "cross-site"),
            JSON,
        ];
        let answer = disconnect(&app, HOST, &headers).await;
        assert_eq!(answer.status, StatusCode::OK, "{origin}: {}", answer.body);
    }
}

#[tokio::test]
async fn a_request_shaped_like_the_clis_still_passes() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let answer = disconnect(&app, HOST, &[JSON]).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
}

#[tokio::test]
async fn the_daemons_own_page_passes_under_a_name_it_answers_to() {
    let access = DaemonAccess::new(None, "127.0.0.1", vec!["gglib.test".into()]);
    let app = shipped(&shipped_cors(), access).await;
    let own = [("origin", "http://gglib.test:9887"), FORM];
    let answer = disconnect(&app, "gglib.test:9887", &own).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let loopback = [("origin", "http://127.0.0.1:9887"), FORM];
    let answer = disconnect(&app, HOST, &loopback).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    // The same page, sending to a host it does not name, is another site.
    assert!(refused(&disconnect(&app, HOST, &own).await));
}

/// DNS rebinding: `evil.com` resolves to this machine, so the page's origin
/// names the very host its request was sent to. The origin guard alone would
/// take that for the daemon's own page; the Host guard outside it is what
/// refuses the name. Every router the daemon can build is checked, since each
/// layers its own Host guard.
#[tokio::test]
async fn a_rebound_page_is_refused_unless_its_name_is_one_the_daemon_answers_to() {
    let cors = shipped_cors();
    let state = test_state(cors.clone()).await;
    let spa_dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-dashboard");
    let routers = |access: DaemonAccess| {
        let access = Arc::new(access);
        let state = || Arc::clone(&state);
        [
            gglib_axum::create_router(state(), &cors, Arc::clone(&access)),
            gglib_axum::create_spa_router(state(), &spa_dir, &cors, Arc::clone(&access)),
            gglib_axum::create_embedded_spa_router(state(), &cors, access),
        ]
    };
    let rebound = [("origin", "http://evil.com:9887"), FORM];
    for app in routers(DaemonAccess::loopback()) {
        let answer = disconnect(&app, "evil.com:9887", &rebound).await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.body);
    }
    let named = DaemonAccess::new(None, "127.0.0.1", vec!["evil.com".into()]);
    for app in routers(named) {
        let answer = disconnect(&app, "evil.com:9887", &rebound).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    }
}

#[tokio::test]
async fn a_cross_site_read_is_left_to_cors() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let path = daemon::REMOTE_STATUS_PATH;
    let answer = send(&app, Method::GET, path, HOST, &[("origin", ELSEWHERE)]).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.allow_origin, None);
}
