//! Tests for [`super::autostart_proxy`]: what the service's start does about
//! the endpoint, for each position of the switch, on a fresh data root.
//!
//! Each context is bootstrapped over a database of its own, as
//! `shutdown_tests.rs` does, and its endpoint is pointed at a free port
//! rather than the 8080 a fresh install would take, so a gglib already
//! running on this machine is neither reached nor in the way.

use std::sync::Arc;

use gglib_core::settings::SettingsUpdate;
use gglib_runtime::proxy::ProxyStatus;

use super::{Autostart, autostart_proxy};
use crate::state::AppState;

/// A context of its own over `dir`, with nothing stored but the port.
async fn fresh(dir: &std::path::Path, proxy_port: u16) -> AppState {
    let state: AppState = Arc::new(
        crate::bootstrap::bootstrap(crate::ServerConfig {
            base_port: Some(19_100),
            llama_server_path: "/nonexistent/llama-server".into(),
            sd_server_path: "/nonexistent/sd-server".into(),
            max_concurrent_agent_loops: 1,
            db_path: Some(dir.join("gglib.db")),
            device_keys_path: Some(dir.join("remote_devices")),
        })
        .await
        .expect("bootstrap an isolated context"),
    );
    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_port: Some(Some(proxy_port)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("store the port");
    state
}

async fn store_switch(state: &AppState, on: bool) {
    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_autostart: Some(Some(on)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("store the switch");
}

/// A port nothing listens on, as the OS hands one out.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port()
}

/// A request the endpoint answers: its model list, empty on a fresh data
/// root.
async fn answers(port: u16) -> bool {
    gglib_proxy::loopback::client_builder()
        .build()
        .expect("a client")
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .send()
        .await
        .is_ok_and(|reply| reply.status().is_success())
}

/// A fresh install stores no switch, and its endpoint is on when the service
/// has started, answering at the stored port.
#[tokio::test]
async fn a_fresh_install_has_the_endpoint_on_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    assert_eq!(
        state.core.settings().get().await.unwrap().proxy_autostart,
        None
    );

    let outcome = autostart_proxy(&state).await;

    assert!(
        matches!(outcome, Autostart::On(addr) if addr.port() == port),
        "{outcome:?}"
    );
    assert!(matches!(
        state.proxy.status().await,
        ProxyStatus::Running { .. }
    ));
    assert!(answers(port).await, "the endpoint answers on {port}");
    state.proxy.stop().await.unwrap();
}

/// A stored `false` is a choice, and the endpoint stays off.
#[tokio::test]
async fn a_stored_false_keeps_the_endpoint_off_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    store_switch(&state, false).await;

    let outcome = autostart_proxy(&state).await;

    assert!(matches!(outcome, Autostart::Off), "{outcome:?}");
    assert!(matches!(state.proxy.status().await, ProxyStatus::Stopped));
}

/// A stored `true` turns the endpoint on, as it always has.
#[tokio::test]
async fn a_stored_true_turns_the_endpoint_on_at_launch() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    store_switch(&state, true).await;

    let outcome = autostart_proxy(&state).await;

    assert!(
        matches!(outcome, Autostart::On(addr) if addr.port() == port),
        "{outcome:?}"
    );
    state.proxy.stop().await.unwrap();
}

/// Another program on the port leaves the endpoint off, and what the service
/// says names the port, so the daemon's log tells the user why.
#[tokio::test]
async fn a_taken_port_leaves_the_endpoint_off_and_says_which_port() {
    let dir = tempfile::tempdir().unwrap();
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = holder.local_addr().unwrap().port();
    let state = fresh(dir.path(), port).await;

    let outcome = autostart_proxy(&state).await;

    let Autostart::Failed(reason) = outcome else {
        panic!("the endpoint cannot come on over a taken port: {outcome:?}");
    };
    assert!(reason.contains(&port.to_string()), "{reason}");
    assert!(!matches!(
        state.proxy.status().await,
        ProxyStatus::Running { .. }
    ));
    // What every surface reads: the status says why the endpoint is off.
    let status = crate::handlers::proxy::status(axum::extract::State(Arc::clone(&state)))
        .await
        .0;
    drop(holder);
    assert!(!status.running);
    assert_eq!(
        status.refused.as_deref(),
        Some(
            format!(
                "The endpoint is off: port {port} is in use by another program. Stop it, or \
                 choose another port (Settings \u{203a} Endpoint, or `gglib config set \
                 proxy-port`), then turn the endpoint on."
            )
            .as_str()
        )
    );
}

/// A service whose endpoint port another gglib service holds, one on another
/// data folder, still comes up, with its endpoint off and saying so.
#[tokio::test]
async fn a_port_another_gglib_service_holds_is_said_to_be_gglibs() {
    let other_dir = tempfile::tempdir().unwrap();
    let other = fresh(other_dir.path(), free_port()).await;
    let Autostart::On(held) = autostart_proxy(&other).await else {
        panic!("the other service's endpoint is on");
    };
    let dir = tempfile::tempdir().unwrap();
    let state = fresh(dir.path(), held.port()).await;

    let outcome = autostart_proxy(&state).await;
    let status = crate::handlers::proxy::status(axum::extract::State(Arc::clone(&state)))
        .await
        .0;
    other.proxy.stop().await.unwrap();

    assert!(matches!(outcome, Autostart::Failed(_)), "{outcome:?}");
    assert!(
        status
            .refused
            .as_deref()
            .is_some_and(|why| why.contains(&format!(
                "port {} is in use by another gglib service",
                held.port()
            ))),
        "{:?}",
        status.refused
    );
}

/// `gglib up` after the launch, with the endpoint stored to listen on every
/// address: up sends no host, so the endpoint is as the settings describe
/// it and stays bound where it is. Naming loopback, as up did, would have
/// asked to rebind it.
#[tokio::test]
async fn up_finds_an_endpoint_the_settings_put_on_every_address_as_asked() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    state
        .core
        .settings()
        .update(SettingsUpdate {
            proxy_host: Some(Some("0.0.0.0".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("store the address");
    assert!(matches!(autostart_proxy(&state).await, Autostart::On(_)));

    let as_up_asks = crate::handlers::proxy::start(
        axum::extract::State(Arc::clone(&state)),
        axum::Json(Some(
            serde_json::from_value(serde_json::json!({ "host": null })).unwrap(),
        )),
    )
    .await;
    let naming_loopback = crate::handlers::proxy::start(
        axum::extract::State(Arc::clone(&state)),
        axum::Json(Some(
            serde_json::from_value(serde_json::json!({ "host": "127.0.0.1" })).unwrap(),
        )),
    )
    .await;
    let bound = state.proxy.status().await;
    state.proxy.stop().await.unwrap();

    assert_eq!(
        as_up_asks.expect("as the settings say").0.outcome,
        Some(gglib_app_services::StartOutcome::Unchanged)
    );
    assert!(naming_loopback.is_err(), "loopback by name differs");
    assert!(
        matches!(bound, ProxyStatus::Running { address } if address.ip().is_unspecified()),
        "{bound:?}"
    );
}

/// The endpoint the launch turns on is said, `proxy_started` with its port,
/// as any start that turns it on is: a window open before it learns it.
#[tokio::test]
async fn the_endpoint_the_launch_turns_on_is_said() {
    use futures_util::StreamExt;
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    let mut events = Box::pin(state.sse.events());

    let outcome = autostart_proxy(&state).await;
    state.proxy.stop().await.unwrap();

    assert!(matches!(outcome, Autostart::On(_)), "{outcome:?}");
    let told = tokio::time::timeout(std::time::Duration::from_secs(5), events.next())
        .await
        .expect("an event")
        .expect("the stream");
    assert_eq!(
        serde_json::to_value(&told).unwrap(),
        serde_json::to_value(gglib_core::events::AppEvent::proxy_started(port)).unwrap()
    );
}

/// `gglib proxy` with no flags, after the launch turned the endpoint on,
/// asks for the endpoint as it runs: nothing changes and nothing is asked.
/// The launch reads the stored endpoint settings one way and a start that
/// names none another, so each is stored here, and must read the same.
#[tokio::test]
async fn a_plain_start_after_the_launch_finds_the_endpoint_as_asked() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let state = fresh(dir.path(), port).await;
    state
        .core
        .settings()
        .update(SettingsUpdate {
            default_context_size: Some(Some(16_384)),
            proxy_cache_on_disk: Some(Some(true)),
            proxy_cache_disk_gb: Some(Some(4)),
            proxy_allowed_hosts: Some(Some(vec!["desk.local".to_owned()])),
            ..SettingsUpdate::default()
        })
        .await
        .expect("store the endpoint's settings");
    assert!(matches!(autostart_proxy(&state).await, Autostart::On(_)));

    let answer =
        crate::handlers::proxy::start(axum::extract::State(Arc::clone(&state)), axum::Json(None))
            .await;
    state.proxy.stop().await.unwrap();

    let status = answer.expect("the endpoint as it runs").0;
    assert_eq!(
        status.outcome,
        Some(gglib_app_services::StartOutcome::Unchanged)
    );
    assert_eq!(status.port, Some(port));
}
