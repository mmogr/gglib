//! Every daemon route the CLI calls must exist here, with the verb it sends.
//!
//! #834 deleted `GET /api/models/downloads` as a route "with no client". The
//! frontend was indeed safe — it polls `/downloads/queue` — but the CLI's
//! download poller was calling the bare mount, and nothing tied the two
//! together, so the whole suite stayed green while `gglib model download`
//! broke in the field.
//!
//! The contract itself lives in `gglib_core::contracts::http::daemon`, which
//! is where the CLI reads its paths from. That shared vocabulary is what lets
//! this test live here, on the server side, without gglib-axum depending on
//! gglib-cli — an edge the boundary checks forbid.
//!
//! ## Why two probes per path
//!
//! Deleting a route rarely produces a clean 404, because some parameterized
//! sibling usually swallows the path. Both observed shapes have to be caught:
//!
//! - `/api/models/downloads/queue` with its route removed matches
//!   `/api/models/downloads/{id}`, which allows only `DELETE`. A `GET` there
//!   answers **405**. Reading the `Allow` header is what exposes it.
//! - `/api/models/downloads` matches `/api/models/{id}`, whose `i64` extractor
//!   answers **400 `Invalid URL: Cannot parse "downloads" to a i64`** — the
//!   original bug, which the poller fed to serde_json and reported as a parse
//!   error at column 1.
//!
//! So: a `TRACE` (which no route registers, so it never runs a handler) reads
//! the matched route's `Allow` list, and a `GET` catches the typed-extractor
//! fallthrough that `Allow` alone cannot see.
//!
//! ## What this still cannot catch
//!
//! A path that falls through to a parameterized sibling supporting the *same*
//! verb is indistinguishable over HTTP from the real thing — `/api/models/{id}`
//! genuinely allows `GET`. Closing that would need router introspection, or a
//! path constant the router itself composes from, which axum's `.nest()`
//! prevents by building relative fragments against absolute client paths.
//! These two probes catch every failure this codebase has actually produced.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon;

/// Send one request and return `(status, allow header, body)`.
async fn probe(app: &axum::Router, method: Method, path: &str) -> (StatusCode, String, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .header("Host", "127.0.0.1:9887")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let allow = response
        .headers()
        .get("allow")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, allow, String::from_utf8_lossy(&bytes).into_owned())
}

/// Check one path, returning a complaint when the daemon does not serve it.
async fn check(app: &axum::Router, methods: &[&str], path: &str) -> Option<String> {
    // TRACE is registered nowhere, so this always stops at method routing and
    // never runs a handler — no side effects even for POST-only routes.
    let (status, allow, _) = probe(app, Method::TRACE, path).await;

    if status == StatusCode::NOT_FOUND {
        return Some(format!("{path}: 404 - no route at this path"));
    }
    if status == StatusCode::METHOD_NOT_ALLOWED {
        let allowed: Vec<&str> = allow.split(',').map(str::trim).collect();
        let missing: Vec<&str> = methods
            .iter()
            .copied()
            .filter(|m| !allowed.contains(m))
            .collect();
        if !missing.is_empty() {
            return Some(format!(
                "{path}: routed, but allows [{allow}] - the CLI sends {missing:?}"
            ));
        }
    }

    // The typed-extractor fallthrough only shows up on a real request.
    if methods.contains(&"GET") {
        let (status, _, body) = probe(app, Method::GET, path).await;
        if status == StatusCode::BAD_REQUEST && body.contains("Invalid URL") {
            return Some(format!(
                "{path}: fell through to a typed path extractor - {body}"
            ));
        }
    }

    None
}

#[tokio::test]
async fn every_daemon_path_the_cli_calls_is_routed() {
    let app = test_app(CorsConfig::AllowAll).await;

    // Collect every failure before asserting, so a broken sweep names them all.
    let mut broken = Vec::new();
    for (methods, path) in daemon::CLI_ROUTE_CONTRACT {
        if let Some(complaint) = check(&app, methods, path).await {
            broken.push(format!("  {complaint}"));
        }
    }
    // The one parameterized path, instantiated from its own definition rather
    // than a second copy of the template.
    let apply = daemon::benchmark_tune_apply_path(1);
    if let Some(complaint) = check(&app, daemon::BENCHMARK_TUNE_APPLY_METHODS, &apply).await {
        broken.push(format!("  {complaint}"));
    }

    assert!(
        broken.is_empty(),
        "the daemon no longer serves {} path(s) the CLI calls:\n{}\n\
         Either restore the route or update gglib_core::contracts::http::daemon.",
        broken.len(),
        broken.join("\n")
    );
}
