//! The daemon's llama-server base port comes from the saved setting (#1161).
//!
//! The daemon builds its config from [`ServerConfig::with_defaults`], and a
//! `Some` base port there outranks the saved `llama_base_port` — which is how
//! every llama-server it started came to begin at 9000 whatever the setting
//! said.

use std::io::Write;
use std::sync::{Arc, Mutex};

use gglib_core::DEFAULT_LLAMA_BASE_PORT;

use super::*;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The line in which a bootstrap with `base_port`, over a database whose
/// saved base port is `saved`, reports the base port it resolved. No
/// accessor exposes the resolved port, so the tests read it from this line.
async fn resolved_base_port(saved: Option<u16>, base_port: Option<u16>) -> String {
    let dir = tempfile::tempdir().unwrap();
    let config = ServerConfig {
        port: 0,
        base_port,
        llama_server_path: "/nonexistent/llama-server".into(),
        db_path: Some(dir.path().join("gglib.db")),
        device_keys_path: Some(dir.path().join("remote_devices")),
        ..ServerConfig::with_defaults().unwrap()
    };
    if let Some(port) = saved {
        let seeding = crate::bootstrap::bootstrap(config.clone()).await.unwrap();
        seeding
            .core
            .settings()
            .update(gglib_core::SettingsUpdate {
                llama_base_port: Some(Some(port)),
                ..gglib_core::SettingsUpdate::default()
            })
            .await
            .unwrap();
        drop(seeding);
    }

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // As in the runs' privacy test: a second registered dispatcher makes
    // callsites other threads hit first consult this one too.
    let capture = tracing::Dispatch::new(subscriber);
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _default = tracing::dispatcher::set_default(&capture);
    tracing::callsite::rebuild_interest_cache();

    crate::bootstrap::bootstrap(config).await.unwrap();

    let log = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    log.lines()
        .find(|line| line.contains("Starting llama-server with base port"))
        .unwrap_or_else(|| panic!("the capture works: {log}"))
        .to_owned()
}

#[test]
fn the_defaults_leave_the_base_port_to_the_saved_setting() {
    assert_eq!(ServerConfig::with_defaults().unwrap().base_port, None);
}

#[tokio::test]
async fn the_daemon_config_starts_llama_servers_from_the_saved_base_port() {
    let daemon = ServerConfig::with_defaults().unwrap().base_port;

    let resolved = resolved_base_port(Some(19_400), daemon).await;

    assert!(resolved.contains("port=19400"), "{resolved}");
    assert!(resolved.contains(r#"source="saved setting""#), "{resolved}");
}

#[tokio::test]
async fn a_base_port_the_caller_names_beats_the_saved_one() {
    let resolved = resolved_base_port(Some(19_400), Some(19_500)).await;

    assert!(resolved.contains("port=19500"), "{resolved}");
    assert!(resolved.contains(r#"source="override""#), "{resolved}");
}

#[tokio::test]
async fn with_neither_the_daemon_starts_llama_servers_from_the_default() {
    let daemon = ServerConfig::with_defaults().unwrap().base_port;

    let resolved = resolved_base_port(None, daemon).await;

    let port = format!("port={DEFAULT_LLAMA_BASE_PORT}");
    assert!(resolved.contains(&port), "{resolved}");
    assert!(resolved.contains(r#"source="default""#), "{resolved}");
}
