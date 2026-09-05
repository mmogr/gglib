//! Request and response bodies for the daemon's HTTP API.
//!
//! One half of a two-sided contract: each type here must serialise compatibly
//! with what the daemon sends or expects, or the CLI silently drops a field.
//! `StartProxyBody` and `ProxyStatusDto` pair with
//! `gglib_axum::handlers::proxy::wire`, and carry the same filename so the
//! pairing is visible from the tree; `StartServerDto` narrows
//! `gglib_app_services::types::StartServerResponse`, and `QueueDownloadBody`
//! pairs with `gglib_axum::handlers::model::downloads`. The tests below pin
//! `StartProxyBody` and `StartServerDto`. `ProxyStatusDto` and
//! `QueueDownloadBody` are not pinned.
//!
//! Split out of `daemon_client/mod.rs`, which owns the *connection* — finding
//! the daemon, launching it, checking its identity. That is a different
//! concern from the shapes travelling over it, and the file sat on the 300 LOC
//! ratchet with new fields due.

use serde::{Deserialize, Serialize};

use gglib_core::ports::PinnedSpec;

/// Body for `POST /api/proxy/start` — the client-side twin of
/// `gglib_axum::handlers::proxy::StartProxyConfig`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StartProxyBody {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub default_context: Option<u64>,
    pub cache: Option<bool>,
    pub slot_dir: Option<std::path::PathBuf>,
    pub pinned: Option<PinnedSpec>,
    pub cache_disk_gb: Option<u64>,
    pub inference_override: Option<gglib_core::domain::InferenceConfig>,
    /// Profile applied to requests naming the pinned model bare.
    pub default_profile: Option<String>,
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
}

/// `GET /api/proxy/status` / start / stop response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProxyStatusDto {
    pub running: bool,
    pub port: Option<u16>,
    #[serde(default)]
    pub pinned_model: Option<String>,
}

/// Body for `POST /api/remote/enable` — the client-side twin of
/// `gglib_axum::handlers::remote::RemoteEnableBody`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct RemoteEnableBody {
    pub allow_mcp: bool,
    pub relay: Option<String>,
    pub discovery: Option<bool>,
}

/// `POST /api/remote/enable` response: the one time the ticket and the code
/// are handed out.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteEnableDto {
    pub ticket: String,
    pub code: String,
    pub pairing: String,
    pub expires_in_s: u64,
}

/// One connected peer in a remote status.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemotePeerDto {
    pub fingerprint: String,
    pub path: String,
}

/// `GET /api/remote/status` and the `disable` response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteStatusDto {
    pub enabled: bool,
    #[serde(default)]
    pub ticket_fingerprint: Option<String>,
    #[serde(default)]
    pub pairing_active: bool,
    #[serde(default)]
    pub paired: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub peers: Vec<RemotePeerDto>,
    #[serde(default)]
    pub mcp_allowed: bool,
    #[serde(default)]
    pub tunnelled_requests: u64,
    #[serde(default)]
    pub last_tunnelled_ms: Option<i64>,
    #[serde(default)]
    pub last_peer: Option<String>,
}

/// `POST /api/servers/start` response.
///
/// A narrowing of `gglib_app_services::types::StartServerResponse`, which also
/// carries a `message` the CLI has no use for. Reading only what is used keeps
/// a daemon of a different build from failing this deserialize over a field
/// nothing renders; `a_start_server_response_deserializes_into_the_narrowing`
/// pins the half that is used.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StartServerDto {
    pub port: u16,
}

/// `POST /api/models/downloads/queue` request body.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct QueueDownloadBody {
    pub model_id: String,
    /// `None` leaves the quantization choice to the daemon.
    pub quant: Option<String>,
}

#[cfg(test)]
mod tests {
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
}
