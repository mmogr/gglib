//! Tests for what the endpoint says while it is off because a start could
//! not bind it: a taken port and what holds it, or the system's reason, in
//! the status and in the start's refusal alike.

use std::sync::Arc;

use gglib_db::{CoreFactory, setup_test_database};
use gglib_runtime::proxy::ProxyConfig;

use crate::error::GuiError;
use crate::test_support::{RecordingEmitter, test_core_and_proxy_heard};

async fn proxy() -> Arc<crate::ProxyOps> {
    let pool = setup_test_database().await.expect("in-memory DB");
    test_core_and_proxy_heard(
        &CoreFactory::build_repos(pool),
        Arc::new(RecordingEmitter::default()),
    )
    .1
}

fn on(port: u16) -> ProxyConfig {
    at("127.0.0.1", port)
}

fn at(host: &str, port: u16) -> ProxyConfig {
    ProxyConfig {
        host: host.into(),
        port,
        ..ProxyConfig::default()
    }
}

/// A port another program holds is said with its number and that it is
/// another program's; a start that binds afterwards clears it.
#[tokio::test]
async fn a_port_another_program_holds_is_said_until_a_start_binds() {
    let proxy = proxy().await;
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = holder.local_addr().unwrap().port();

    let refused = proxy.start(on(port), None).await;
    let said = proxy.refused();
    let bound = proxy.start(on(0), None).await;
    let cleared = proxy.refused();
    proxy.stop().await.unwrap();
    drop(holder);

    assert!(refused.is_err());
    assert_eq!(
        said.as_deref(),
        Some(
            format!(
                "The endpoint is off: port {port} is in use by another program. Stop it, or \
                 choose another port (Settings \u{203a} Endpoint, or `gglib config set \
                 proxy-port`), then turn the endpoint on."
            )
            .as_str()
        )
    );
    assert!(bound.is_ok());
    assert_eq!(cleared, None);
}

/// The reason is said when it is known, `proxy_stopped`, so a window that
/// read the status before the bind failed reads it again and sees why.
#[tokio::test]
async fn a_refused_bind_is_said_so_the_status_is_read_again() {
    let pool = setup_test_database().await.expect("in-memory DB");
    let heard = Arc::new(RecordingEmitter::default());
    let (_core, proxy) = test_core_and_proxy_heard(
        &CoreFactory::build_repos(pool),
        Arc::clone(&heard) as Arc<dyn gglib_core::ports::AppEventEmitter>,
    );
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = holder.local_addr().unwrap().port();

    let refused = proxy.start(on(port), None).await;
    drop(holder);

    assert!(refused.is_err());
    assert!(proxy.refused().is_some());
    let said: Vec<String> = heard
        .events()
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    assert_eq!(
        said,
        [serde_json::to_string(&gglib_core::events::AppEvent::proxy_stopped()).unwrap()]
    );
}

/// A second gglib service holding the port is named as one.
#[tokio::test]
async fn a_port_another_gglib_holds_is_said_to_be_gglibs() {
    let other = proxy().await;
    let port = other
        .start(on(0), None)
        .await
        .expect("the other gglib")
        .port();
    let proxy = proxy().await;

    let refused = proxy.start(on(port), None).await;
    let said = proxy.refused();
    other.stop().await.unwrap();

    assert!(refused.is_err());
    assert!(
        said.as_deref()
            .is_some_and(|s| s.contains(&format!("port {port} is in use by another gglib service"))),
        "{said:?}"
    );
}

/// A start refused because its port is taken says the status's sentence,
/// word for word: one sentence, not a second wording of the same fact.
#[tokio::test]
async fn a_start_refused_for_a_taken_port_says_the_statuss_sentence() {
    let proxy = proxy().await;
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = holder.local_addr().unwrap().port();

    let refused = proxy.start(on(port), None).await;
    let said = proxy.refused().expect("the status says why");
    drop(holder);

    let Err(GuiError::Conflict(message)) = refused else {
        panic!("refused as a conflict: {refused:?}");
    };
    assert_eq!(message, said);
}

/// An address this machine does not have (TEST-NET-1) is said with the
/// system's own reason and what to change, in the status and the refusal
/// alike, and no other program is blamed.
#[tokio::test]
async fn an_address_this_machine_cannot_open_is_said_with_the_systems_reason() {
    let proxy = proxy().await;
    let holder = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = holder.local_addr().unwrap().port();
    let reason = std::net::TcpListener::bind(("192.0.2.1", port))
        .expect_err("this machine does not have 192.0.2.1")
        .to_string();

    let refused = proxy.start(at("192.0.2.1", port), None).await;
    let said = proxy.refused();
    drop(holder);

    let expected = format!(
        "The endpoint is off: 192.0.2.1:{port} cannot be opened ({reason}). Choose an address \
         this machine has, and a port it may open (Settings \u{203a} Endpoint, or `gglib \
         config set proxy-host` and `proxy-port`), then turn the endpoint on."
    );
    assert_eq!(said.as_deref(), Some(expected.as_str()));
    let Err(GuiError::Conflict(message)) = refused else {
        panic!("refused as a conflict: {refused:?}");
    };
    assert_eq!(message, expected);
    assert!(!message.contains("another program"), "{message}");
}
