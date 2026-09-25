//! Tests for the daemon wire bodies: the keys `StartProxyBody` puts on the
//! wire, and the start-server narrowing read from what the daemon sends.

use super::*;
use gglib_core::contracts::http::daemon::PROXY_START_CLI_FIELDS;

/// The keys the CLI actually puts on the wire, against the shared list the
/// daemon's own test reads.
///
/// The literal is exhaustive on purpose — no `..Default::default()`. A field
/// added to `StartProxyBody` fails to compile here rather than reaching the
/// daemon unannounced.
#[test]
fn a_populated_start_body_sends_exactly_the_contract_fields() {
    let body = StartProxyBody {
        host: Some("127.0.0.1".into()),
        port: Some(8080),
        default_context: Some(4096),
        cache: Some(true),
        slot_dir: Some("/slots".into()),
        pinned: Some(PinnedSpec::default()),
        cache_disk_gb: Some(8),
        inference_override: Some(gglib_core::domain::InferenceConfig::default()),
        default_profile: Some("fast".into()),
        api_key: Some("k".into()),
        // Non-empty deliberately: an empty vec puts no key on the wire at
        // all, which the next test covers.
        allowed_hosts: vec!["example.test".into()],
    };

    let json = serde_json::to_value(&body).expect("StartProxyBody serialises");
    let mut got: Vec<String> = json
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect();
    got.sort();
    let mut want: Vec<String> = PROXY_START_CLI_FIELDS
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    want.sort();

    assert_eq!(got, want, "CLI body keys have drifted from the contract");
}

/// `gglib up`, and any `gglib proxy` without `--allowed-host`, send no
/// `allowed_hosts` key at all. The daemon's `#[serde(default)]` on that
/// field is what absorbs it — without which those calls would 422 rather
/// than degrade.
#[test]
fn an_empty_allowed_hosts_omits_the_key() {
    let json = serde_json::to_value(StartProxyBody::default()).expect("serialises");
    assert!(
        !json
            .as_object()
            .expect("a JSON object")
            .contains_key("allowed_hosts"),
        "an empty allowed_hosts must not put a key on the wire"
    );
}

/// The start-server narrowing, both sides in one test: gglib-cli can see
/// the daemon's response type, so this pins behaviour rather than names.
#[test]
fn a_start_server_response_deserializes_into_the_narrowing() {
    let sent = gglib_app_services::types::StartServerResponse {
        port: 8081,
        message: "Server started on port 8081".into(),
    };

    let json = serde_json::to_value(&sent).expect("response serialises");
    let got: StartServerDto =
        serde_json::from_value(json).expect("the narrowing reads what the daemon sends");

    assert_eq!(got.port, sent.port);
}
