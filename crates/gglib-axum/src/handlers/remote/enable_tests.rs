//! `POST /api/remote/enable` refuses a name `--allow-mcp` cannot open before
//! anything is armed or stored.

use axum::Json;
use axum::extract::State;
use gglib_app_services::RemoteEnableBody;
use gglib_core::SettingsUpdate;

use super::enable;
use crate::error::HttpError;
use crate::handlers::agent::run_fixture::state;

/// A name no configured server has is a `400` that names it, and the switch
/// stays off: no session, no stored flags. An `--invite` alongside is
/// checked the same: it may be the call that arms the session.
#[tokio::test]
async fn an_enable_naming_no_server_is_refused_and_switches_nothing_on() {
    // A check that let a name through would arm, so the arm is kept off this
    // machine's gglib folder and its proxy port.
    gglib_core::paths::isolate_data_root();
    for invite in [false, true] {
        let (_dir, state) = state().await;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .expect("a free port")
            .port();
        state
            .core
            .settings()
            .update(SettingsUpdate {
                proxy_port: Some(Some(port)),
                ..SettingsUpdate::default()
            })
            .await
            .expect("settings update");

        // Off the network, so a check that let this through would arm
        // without reaching anything.
        let body = RemoteEnableBody {
            mcp_servers: vec!["nope".to_owned()],
            discovery: Some(false),
            invite,
            ..RemoteEnableBody::default()
        };
        let refused = enable(State(state.clone()), Json(Some(body))).await;

        assert!(
            matches!(&refused, Err(HttpError::BadRequest(m)) if m.contains("'nope'")),
            "invite={invite}: {:?}",
            refused.err()
        );
        let settings = state.core.settings().get().await.unwrap();
        assert_ne!(settings.remote_enabled, Some(true), "the switch stays off");
        assert!(settings.remote_serve.is_none(), "and no flags are stored");
        assert!(!state.remote.status().await.enabled, "nor is a tunnel up");
    }
}
