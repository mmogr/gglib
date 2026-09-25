//! Which routes the origin guard stands in front of, walked (#1118).
//!
//! `daemon_origin.rs` shows the guard's rules on one route. These walk the
//! routes a page on another site could reach, and check that the CORS layer
//! and the guard agree on who may do what.

mod common;

use axum::http::{Method, StatusCode};

use common::harness::{test_access, test_state};
use common::origin::{ELSEWHERE, FORM, HOST, send, shipped, shipped_cors};
use gglib_axum::{CorsConfig, DaemonAccess};
use gglib_core::contracts::http::daemon;

/// Every change the CLI's route contract names, sent the way a page on
/// another site would send it. Derived from the contract, so a route added
/// there is walked here without anyone listing it twice.
#[tokio::test]
async fn every_change_the_cli_can_make_is_refused_to_another_site() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let mut changes: Vec<(&str, String)> = daemon::CLI_ROUTE_CONTRACT
        .iter()
        .flat_map(|(methods, path)| methods.iter().map(move |m| (*m, (*path).to_owned())))
        .filter(|(method, _)| *method != "GET")
        .collect();
    let apply = daemon::benchmark_tune_apply_path(1);
    let forget = daemon::remote_forget_path("dev-0a1b2c3d");
    for (methods, path) in [
        (daemon::BENCHMARK_TUNE_APPLY_METHODS, apply),
        (daemon::REMOTE_FORGET_METHODS, forget),
    ] {
        changes.extend(methods.iter().map(|m| (*m, path.clone())));
    }
    assert!(changes.len() >= 15, "the walk found only {changes:?}");

    let mut let_through = Vec::new();
    for (method, path) in &changes {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        let headers = [("origin", ELSEWHERE), FORM];
        let answer = send(&app, method.clone(), path, HOST, &headers).await;
        if !answer.refused_for_its_origin() {
            let_through.push(format!("  {method} {path}: {}", answer.status));
        }
    }
    assert!(
        let_through.is_empty(),
        "another site reached:\n{}",
        let_through.join("\n")
    );
}

/// The routes #1118 lists that the CLI's contract does not, the llama.cpp
/// install, update and uninstall and the Python setup among them. Walked
/// with `PATCH`, which none of them registers. The guard wraps each
/// registered path's method router, so a method the path does not register
/// meets the guard before the router's 405, and a path that is not registered
/// answers 404. A refusal here shows the guard stands in front of each path;
/// a guard that stopped doing so would get a 405, never a handler that acts
/// on the machine running the test.
#[tokio::test]
async fn the_routes_that_act_on_this_machine_are_refused_to_another_site() {
    const ISSUE_1118: [&str; 16] = [
        "/api/config/system/install-llama",
        "/api/config/system/update-llama",
        "/api/config/system/uninstall-llama",
        "/api/config/system/setup-python",
        "/api/config/system/disable-fast-downloads",
        "/api/config/system/llama-check-updates",
        "/api/mcp/servers/1/start",
        "/api/mcp/servers/1/stop",
        "/api/mcp/servers/1/test",
        "/api/mcp/servers/1/resolve",
        "/api/models/1/tags/pinned",
        "/api/models/1/upgrade",
        "/api/models/1/verify",
        "/api/models/downloads/1/cancel",
        "/api/models/downloads/shard-group/1/cancel",
        "/api/models/downloads/failed/clear",
    ];
    let cors = shipped_cors();
    let app = gglib_axum::create_router(test_state(cors.clone()).await, &cors, test_access());
    let mut let_through = Vec::new();
    for path in ISSUE_1118 {
        let headers = [("origin", ELSEWHERE), FORM];
        let answer = send(&app, Method::PATCH, path, HOST, &headers).await;
        if !answer.refused_for_its_origin() {
            let_through.push(format!("  {path}: {}", answer.status));
        }
    }
    assert!(
        let_through.is_empty(),
        "another site reached:\n{}",
        let_through.join("\n")
    );
}

/// Under every config, a page that names any origin but the daemon's own may
/// change something exactly when the CORS layer lets it read the answer.
/// Read with `GET /api/version`, changed with the in-memory
/// `POST /api/models/downloads/failed/clear`.
#[tokio::test]
async fn every_origin_the_daemon_lets_read_may_change_things_and_no_other() {
    let origins = [
        ELSEWHERE,
        "http://localhost:3000",
        "http://localhost:5173",
        "tauri://localhost",
        "http://tauri.localhost",
        "http://192.168.1.5:9887",
    ];
    let clear = "/api/models/downloads/failed/clear";
    for cors in [CorsConfig::AllowAll, shipped_cors(), CorsConfig::LocalOnly] {
        let app = shipped(&cors, DaemonAccess::loopback()).await;
        let mut readers = 0;
        for origin in origins {
            let path = daemon::VERSION_PATH;
            let read = send(&app, Method::GET, path, HOST, &[("origin", origin)]).await;
            let headers = [("origin", origin), FORM];
            let write = send(&app, Method::POST, clear, HOST, &headers).await;
            let reads = read.allow_origin.is_some();
            let writes = !write.refused_for_its_origin();
            assert_eq!(writes, reads, "{cors:?}: {origin} reads {reads}");
            assert!(
                !writes || write.status == StatusCode::OK,
                "{origin}: {}",
                write.body
            );
            readers += usize::from(reads);
        }
        // Neither side of the comparison is empty where a config has two:
        // each lets some origin read, and only AllowAll lets all of them.
        assert!(readers > 0, "{cors:?} let no origin read");
        let all = readers == origins.len();
        assert_eq!(all, cors == CorsConfig::AllowAll, "{cors:?}");
    }
}
