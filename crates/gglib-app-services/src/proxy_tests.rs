use super::*;

// ---------------------------------------------------------------
// ensure_running — settings-aware port, and BindFailed as Conflict
// ---------------------------------------------------------------

/// `ensure_running` must bind the saved `proxy_port`, not the hardcoded
/// `ProxyConfig::default()` port — otherwise a user with a standing
/// `gglib serve`/`gglib proxy` on a non-default port would still collide
/// on 8080 the moment the GUI starts a model.
#[tokio::test]
async fn ensure_running_uses_the_saved_proxy_port_setting() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;

    let saved_port = 18080;
    core.settings()
        .update(gglib_core::SettingsUpdate {
            proxy_port: Some(Some(saved_port)),
            ..Default::default()
        })
        .await
        .expect("settings update should succeed");

    let addr = proxy
        .ensure_running()
        .await
        .expect("ensure_running should succeed on an unused port");
    assert_eq!(addr.port(), saved_port);

    proxy.stop().await.expect("stop should succeed");
}

/// A foreign process already holding the configured port must surface as
/// a `Conflict` naming the port — not `Internal`, and not the confusing
/// "reported as already running but status is Stopped" message that
/// `ensure_running`'s self-race recovery would produce if `BindFailed`
/// were routed through it: this supervisor never started the foreign
/// process, so its own status correctly stays `Stopped` throughout.
#[tokio::test]
async fn ensure_running_reports_a_clear_conflict_when_the_port_is_taken_by_another_process() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;

    // Hold the port ourselves to simulate a foreign gglib process.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind a port for the test");
    let taken_port = listener.local_addr().unwrap().port();

    core.settings()
        .update(gglib_core::SettingsUpdate {
            proxy_port: Some(Some(taken_port)),
            ..Default::default()
        })
        .await
        .expect("settings update should succeed");

    let err = proxy
        .ensure_running()
        .await
        .expect_err("a taken port must not be reported as success");

    let GuiError::Conflict(message) = &err else {
        panic!("expected Conflict, got {err:?}");
    };
    assert!(
        message.contains(&taken_port.to_string()),
        "conflict message should name the port: {message}"
    );

    drop(listener);
}

/// A proxy this starts carries the runs it was handed, so `/v1/runs` is
/// served rather than answered 503.
#[tokio::test]
async fn a_started_proxy_serves_the_runs_it_was_handed() {
    let (_, proxy) = crate::test_support::test_core_and_proxy().await;
    let (runs, _, _) = crate::runs::test_executor::registry();
    let runs: Arc<dyn gglib_core::ports::RunsPort> = Arc::new(runs);
    proxy.bind_runs(&runs);

    let config = ProxyConfig {
        port: 0,
        ..ProxyConfig::default()
    };
    let addr = proxy.start(config, None).await.expect("the proxy starts");
    let status = gglib_proxy::loopback::client()
        .get(format!("http://{addr}/v1/runs"))
        .send()
        .await
        .expect("the proxy answers")
        .status();
    proxy.stop().await.expect("stop");

    assert_eq!(status.as_u16(), 200);
}
