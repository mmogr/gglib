//! Tests for [`super::await_shutdown`] and [`super::teardown`].
//!
//! Split out via `#[path]`, as this repo's other test modules are.

use super::*;

/// The signal path must cancel the token, not merely observe it.
///
/// Without this, everything bounded by the token — `/api/events` above all —
/// never ends, so `with_graceful_shutdown` never returns and `perform_shutdown`
/// never runs. Ctrl-C, `systemctl stop` and `kill` all take this path, so the
/// bug hid behind the one trigger that did work: the API route, which cancels
/// the token itself.
#[tokio::test]
async fn the_signal_path_cancels_the_token() {
    let token = CancellationToken::new();
    await_shutdown(std::future::ready(()), token.clone()).await;
    assert!(token.is_cancelled());
}

/// The API path already cancels the token before this future is polled; firing
/// it again must be harmless, which is what lets the cancel be unconditional.
#[tokio::test]
async fn the_api_path_tolerates_a_second_cancel() {
    let token = CancellationToken::new();
    token.cancel();
    await_shutdown(std::future::pending(), token.clone()).await;
    assert!(token.is_cancelled());
}

/// The daemon's teardown writes what the loop guard recorded — through the
/// chain the daemon builds, from the bootstrap's writer to the drain, so a link
/// left out anywhere in it fails here: the bootstrap's writer, the service
/// graph's supervisor, the proxy's step with the session id, and the drain.
/// `perform_shutdown`'s call to `teardown`, and `run_daemon`'s to
/// `perform_shutdown`, are outside it and are held by review: the watchdog and
/// the pidfile audit have no place in a test.
///
/// The request names a model that does not exist, so nothing is sent
/// upstream: the step records before admission, lets the request go on under
/// the default mode, and admission's 404 for an unknown model is what the
/// client gets.
#[tokio::test]
async fn teardown_writes_what_the_loop_guard_recorded() {
    use gglib_core::domain::loop_guard_log::epoch_day;
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gglib.db");
    let state: AppState = std::sync::Arc::new(
        crate::bootstrap::bootstrap(crate::ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            base_port: 19_000,
            llama_server_path: "/nonexistent/llama-server".into(),
            max_concurrent_agent_loops: 1,
            static_dir: None,
            cors: gglib_core::CorsConfig::AllowAll,
            db_path: Some(db.clone()),
            device_keys_path: Some(dir.path().join("remote_devices")),
        })
        .await
        .expect("bootstrap an isolated context"),
    );
    let addr = state
        .proxy
        .start(
            gglib_runtime::proxy::ProxyConfig {
                host: "127.0.0.1".into(),
                port: 0,
                ..Default::default()
            },
            None,
        )
        .await
        .expect("start the proxy on a free port");

    // Three identical batches of a mutating tool, each answered the same way.
    let call = json!({ "role": "assistant", "content": null, "tool_calls": [{
        "id": "c1", "type": "function",
        "function": { "name": "write_file", "arguments": "{\"path\":\"a\"}" }
    }] });
    let answer = json!({ "role": "tool", "tool_call_id": "c1", "content": "1 file changed" });
    let mut messages = vec![json!({ "role": "user", "content": "go" })];
    for _ in 0..3 {
        messages.push(call.clone());
        messages.push(answer.clone());
    }
    messages.push(json!({ "role": "user", "content": "continue" }));
    gglib_proxy::loopback::client_builder()
        .build()
        .unwrap()
        .post(format!("http://{addr}/v1/chat/completions"))
        .header("x-gglib-session-id", "s1")
        .json(&json!({ "model": "no-such-model", "messages": messages }))
        .send()
        .await
        .expect("the proxy answers");

    teardown(&state).await;

    // Read back through the context's own reader — the one the route answers
    // from — over the database the writer wrote to.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = state
        .loop_guard_trips
        .summary(epoch_day(now) - 1)
        .await
        .unwrap();
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(days[0].mode, gglib_core::LoopGuardMode::Note);
    assert_eq!(
        (
            days[0].scanned,
            days[0].trips,
            days[0].loops,
            days[0].sessions
        ),
        (1, 1, 1, 1)
    );
}
