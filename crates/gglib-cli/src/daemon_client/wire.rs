//! Request and response bodies for the daemon's HTTP API.
//!
//! One half of a two-sided contract: each type here must serialise compatibly
//! with what the daemon sends or expects, or the CLI silently drops a field.
//! `StartProxyBody` and `ProxyStatusDto` pair with
//! `gglib_axum::handlers::proxy::wire`, and carry the same filename so the
//! pairing is visible from the tree; `StartServerDto` narrows
//! `gglib_app_services::types::StartServerResponse`, and `QueueDownloadBody`
//! pairs with `gglib_axum::handlers::model::downloads`. The tests in
//! `wire_tests.rs` pin `StartProxyBody` and `StartServerDto`.
//! `ProxyStatusDto` and `QueueDownloadBody` are not pinned.
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

/// One row of `GET /api/remote/devices`.
///
/// A narrowing of `gglib_axum::handlers::remote::RemoteDevice`: every field
/// the terminal renders. A field a daemon older than this client does not
/// send reads as `None`, or as zero for `joined_at`; only `id` is required.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteDeviceDto {
    /// The name the edge holds the key under, and what `forget` takes.
    pub id: String,
    /// What the device called itself at join, when it said.
    #[serde(default)]
    pub label: Option<String>,
    /// Unix milliseconds at which the invite was minted.
    #[serde(default)]
    pub joined_at: i64,
    /// Unix milliseconds at which a device redeemed it, or `None` if none
    /// ever did — an invite nobody took.
    #[serde(default)]
    pub redeemed_at: Option<i64>,
    /// Unix milliseconds of the last request under its key.
    #[serde(default)]
    pub last_seen: Option<i64>,
    /// The fingerprint of the endpoint that redeemed it, when recorded.
    pub peer: Option<String>,
    /// Whether the edge admits it now, or `None` with the tunnel down.
    #[serde(default)]
    pub admitted: Option<bool>,
    /// Whether the roster lists it; `Some(false)` is a key held with no row.
    /// `None` from a daemon older than this client, which listed rows only.
    #[serde(default)]
    pub recorded: Option<bool>,
}

/// `DELETE /api/remote/devices/{device}` response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteForgottenDto {
    /// Whether this machine held anything under that name.
    #[serde(default)]
    pub forgotten: bool,
}

/// Body for `POST /api/remote/enable` — the client-side twin of
/// `gglib_axum::handlers::remote::RemoteEnableBody`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct RemoteEnableBody {
    pub allow_mcp: bool,
    pub relay: Option<String>,
    pub discovery: Option<bool>,
    pub invite: bool,
}

/// `POST /api/remote/enable` response: the one time the ticket and the code
/// are handed out.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteEnableDto {
    pub ticket: String,
    /// Present only when `enable` was asked to offer a code. Absent is the
    /// ordinary case; rendering it as an empty string would show a pairing
    /// that had already expired rather than none at all.
    pub code: Option<String>,
    pub pairing: Option<String>,
    pub expires_in_s: Option<u64>,
    /// What the daemon says about `/mcp` on the session it acted on, which
    /// is not always the one this call asked for: `--invite` against a
    /// tunnel that is already up leaves the flags alone.
    pub mcp_allowed: bool,
    /// Whether the tunnel was already up and this call answered from that
    /// session. The flags sent with it were ignored if so, which is the only
    /// way to know that `--allow-mcp` did not take.
    #[serde(default)]
    pub already_up: bool,
}

/// `POST /api/remote/connect` request body. Mirrors
/// `gglib_axum::handlers::remote::RemoteConnectBody`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct RemoteConnectBody {
    pub pairing: Option<String>,
    pub port: Option<u16>,
    pub relay: Option<String>,
    pub discovery: Option<bool>,
}

/// `POST /api/remote/connect` response.
///
/// A narrowing of `gglib_axum::handlers::remote::RemoteConnectResponse`,
/// which also carries the bare `port`; the CLI prints the URL, which has it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteConnectDto {
    pub base_url: String,
    pub ticket_fingerprint: String,
    pub paired: bool,
    /// The port this machine wanted and could not have, when it moved.
    #[serde(default)]
    pub moved_from: Option<u16>,
}

/// The connect side in a remote status, while it is up. Narrowed like
/// `RemoteConnectDto`: the URL carries the port.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RemoteConnectionDto {
    pub base_url: String,
    pub ticket_fingerprint: String,
    pub path: String,
    /// Seconds the far machine has been away, or `None` while it is here.
    #[serde(default)]
    pub away_for_s: Option<u64>,
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
    #[serde(default)]
    pub connected: Option<RemoteConnectionDto>,
    #[serde(default)]
    pub stored_ticket_fingerprint: Option<String>,
    #[serde(default)]
    pub has_remote_key: bool,
    /// Whether this machine comes back reachable after a restart.
    #[serde(default)]
    pub remote_enabled: bool,
    /// Where the endpoint key is kept; deleting it retires this machine's
    /// address and revokes no device.
    #[serde(default)]
    pub identity_path: Option<String>,
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
#[path = "wire_tests.rs"]
mod wire_tests;
