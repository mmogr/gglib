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
//! The remote tunnel's shapes are not here: the CLI reads and sends
//! `gglib_app_services`' own `RemoteStatus`, `RemoteDevice` and the rest, the
//! types the daemon reads and answers with.
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
