//! Tests for the proxy arm's runtime port and settings in
//! `proxy_arm_ports.rs`, and its lifecycle in `proxy_arm.rs`.

use gglib_core::LoopGuardMode;
use gglib_core::ports::{LaunchOverrides, ModelRuntimeError, ModelRuntimePort, SettingsRepository};

use super::super::mock_upstream::{MockUpstream, NoCatalog};
use super::*;

fn target() -> RunningTarget {
    RunningTarget::local(1, 1, "held".to_owned(), 4096, false)
}

/// The proxy admits per request, and on a dead upstream it stops the model
/// and admits again. Against the eval's held model, it may do neither.
#[tokio::test]
async fn the_pinned_target_never_launches_or_stops_a_model() {
    let port = PinnedTarget { target: target() };

    let admission = port
        .admit("held", Some(131_072), None, LaunchOverrides::default())
        .await
        .expect("the held model is admitted");
    assert_eq!(admission.target.base_url, target().base_url);
    assert_eq!(
        admission.target.effective_ctx, 4096,
        "a context request relaunches nothing"
    );

    let refused = port
        .admit("other", None, None, LaunchOverrides::default())
        .await
        .expect_err("another model is refused");
    assert!(matches!(
        refused,
        ModelRuntimeError::PinnedModelMismatch { .. }
    ));

    assert!(
        port.stop_current().await.is_err(),
        "it must not stop the held model"
    );
    assert!(port.current_model().await.is_none());
}

/// What the proxy reads: client sampling trusted, so seeds survive, repair on,
/// the loop guard in `note` mode, and no global sampling defaults.
#[tokio::test]
async fn the_fixed_settings_trust_client_sampling_and_add_no_defaults() {
    let settings = FixedSettings::for_eval().load().await.expect("loads");
    assert_eq!(settings.trust_client_sampling, Some(true));
    assert_eq!(settings.tool_call_repair, Some(true));
    assert_eq!(settings.effective_loop_guard_mode(), LoopGuardMode::Note);
    assert!(settings.inference_defaults.is_none());
}

/// The loop-guard mode the report records is the mode the fixed settings
/// give. A copy that happened to name the same mode would pass too.
#[tokio::test]
async fn the_report_records_the_loop_guard_mode_the_proxy_reads() {
    let upstream = MockUpstream::spawn().await;
    let held = RunningTarget::local(upstream.port, 1, "held".to_owned(), 4096, false);
    let arm = ProxyArm::start(held, Arc::new(NoCatalog))
        .await
        .expect("starts");
    let (_, recorded) = arm.finish().await;
    let read = FixedSettings::for_eval().load().await.expect("loads");
    assert_eq!(recorded.loop_guard_mode, read.effective_loop_guard_mode());
}

/// Every early return in the eval drops the arm, and the drop has to stop the
/// proxy, or a failed eval leaves a proxy listening on loopback.
#[tokio::test]
async fn the_proxy_stops_when_the_arm_is_dropped() {
    let upstream = MockUpstream::spawn().await;
    let held = RunningTarget::local(upstream.port, 1, "held".to_owned(), 4096, false);
    let arm = ProxyArm::start(held, Arc::new(NoCatalog))
        .await
        .expect("starts");
    let addr = arm.base_url().trim_start_matches("http://").to_owned();
    tokio::net::TcpStream::connect(&addr)
        .await
        .expect("listening while held");

    drop(arm);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::net::TcpStream::connect(&addr).await.is_ok() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "still listening after the drop"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
