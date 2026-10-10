//! An `sd-server` is asked whether it is up at `/v1/models`, its body must
//! name `sd-cpp-local`, and it reads as healthy while a render holds it.
//!
//! Against [`FakeSdServer`](super::fake_server::FakeSdServer); llama-server's
//! `/health` behaviour is pinned in `health_monitor_tests.rs`, unchanged.

use std::time::Duration;

use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{ProcessHandle, ServerHealthStatus};

use super::fake_server::FakeSdServer;
use crate::health_monitor::ServerHealthChecker;
use crate::process::{check_http_health, wait_for_http_health};

/// The longest a probe may take while a render runs: the health client's own
/// timeout. A probe that needed longer would be reported unhealthy and the
/// drawing server recycled mid-render.
const PROBE_BOUND: Duration = Duration::from_secs(2);

/// Start a render against `port` that the fake will never answer, and wait
/// until the fake is holding it.
async fn hold_a_render(fake: &FakeSdServer) -> tokio::task::JoinHandle<()> {
    let url = format!("http://127.0.0.1:{}/sdcpp/v1/img_gen", fake.port);
    let render = tokio::spawn(async move {
        let client = gglib_proxy::loopback::client_builder()
            .build()
            .expect("a client with no timeout");
        let _ = client
            .post(url)
            .body(r#"{"prompt":"a cat","width":1024,"height":1024}"#)
            .send()
            .await;
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while fake.renders_held() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the fake took the render");
    render
}

#[tokio::test]
async fn sd_server_answers_its_probe_while_a_render_is_held() {
    let fake = FakeSdServer::serve().await;
    let render = hold_a_render(&fake).await;

    for probe in 1..=3 {
        let healthy = tokio::time::timeout(
            PROBE_BOUND,
            check_http_health(fake.port, RuntimeKind::StableDiffusion),
        )
        .await
        .unwrap_or_else(|_| panic!("probe {probe} took longer than {PROBE_BOUND:?}"));
        assert!(
            healthy,
            "probe {probe} read a drawing sd-server as unhealthy"
        );
    }

    assert_eq!(fake.renders_held(), 1, "the render is still being drawn");
    assert!(!render.is_finished(), "nothing answered the render");
    render.abort();
}

#[tokio::test]
async fn the_monitor_asks_an_sd_handle_the_sd_way() {
    let fake = FakeSdServer::serve().await;
    let render = hold_a_render(&fake).await;

    let handle = ProcessHandle::new(7, "flux".to_owned(), None, fake.port, 0)
        .with_runtime(RuntimeKind::StableDiffusion);
    assert_eq!(
        ServerHealthChecker::check_combined(&handle).await,
        ServerHealthStatus::Healthy
    );

    // The same server asked as a llama-server: it has no `/health`.
    let as_llama = ProcessHandle::new(7, "flux".to_owned(), None, fake.port, 0);
    assert!(matches!(
        ServerHealthChecker::check_combined(&as_llama).await,
        ServerHealthStatus::Unreachable { .. }
    ));
    render.abort();
}

#[tokio::test]
async fn a_server_that_does_not_list_sd_cpp_local_is_not_sd_server() {
    let foreign = FakeSdServer::serve_models_body(r#"{"data":[]}"#).await;

    assert!(!check_http_health(foreign.port, RuntimeKind::StableDiffusion).await);
    assert_eq!(
        ServerHealthChecker::check_http(foreign.port, RuntimeKind::StableDiffusion).await,
        ServerHealthStatus::Unreachable {
            last_error: "HTTP health check returned non-success status".to_owned(),
        }
    );
}

#[tokio::test]
async fn a_launch_waits_for_sd_server_at_v1_models() {
    let fake = FakeSdServer::serve().await;

    wait_for_http_health(fake.port, 10, RuntimeKind::StableDiffusion)
        .await
        .expect("the fake sd-server is ready");
}

#[tokio::test]
async fn a_launch_does_not_take_a_foreign_server_for_sd_server_and_says_sd_server() {
    let foreign = FakeSdServer::serve_models_body(r#"{"data":[]}"#).await;

    let err = wait_for_http_health(foreign.port, 2, RuntimeKind::StableDiffusion)
        .await
        .expect_err("a server without sd-cpp-local is never ready");
    let message = err.to_string();
    assert!(message.contains("sd-server"), "{message}");
    assert!(!message.contains("llama-server"), "{message}");
}
