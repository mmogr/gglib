//! Tests for the endpoint's switch at launch as the settings page reads it:
//! [`super::SettingsOps`] over a real database, driven with the JSON the page
//! sends, answering what the service does at its start.

use std::sync::Arc;

use super::*;
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

fn make_ops(core: Arc<AppCore>) -> SettingsOps {
    SettingsOps::new(SettingsDeps {
        core,
        system_probe: Arc::new(MockSystemProbePort::default()),
        downloads: Arc::new(MockDownloadManager::new()),
    })
}

async fn write(ops: &SettingsOps, json: &str) -> serde_json::Value {
    let request: UpdateSettingsRequest = serde_json::from_str(json).expect("a request");
    let answer = ops.update(request).await.expect("stored");
    serde_json::to_value(answer).expect("serializes")["proxyAutostart"].clone()
}

/// A fresh install reads as on, because the service turns the endpoint on;
/// a stored `false` reads as off, a stored `true` as on, and clearing it
/// reads as on again.
#[tokio::test]
async fn the_page_reads_what_the_service_does_at_launch() {
    let ops = make_ops(test_core().await);
    let fresh = serde_json::to_value(ops.get().await.expect("read")).expect("serializes");
    assert_eq!(fresh["proxyAutostart"], serde_json::json!(true));

    assert_eq!(write(&ops, r#"{"proxyAutostart": false}"#).await, false);
    assert_eq!(write(&ops, r#"{"proxyAutostart": true}"#).await, true);
    assert_eq!(write(&ops, r#"{"proxyAutostart": null}"#).await, true);
}

/// "Nothing configured" is a fresh install, whose endpoint comes on.
#[test]
fn nothing_configured_reads_as_on() {
    let wire = serde_json::to_value(AppSettings::default()).expect("serializes");
    assert_eq!(wire["proxyAutostart"], serde_json::json!(true));
}
