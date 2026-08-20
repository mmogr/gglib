//! Request and response bodies for the daemon's HTTP API.
//!
//! One half of a two-sided contract: each type here is the client-side twin of
//! a type in `gglib_axum::handlers::proxy::wire`, and the two must serialise
//! compatibly or the CLI silently sends a field the daemon drops. They carry
//! the same filename as that module so the pairing is visible from the tree.
//!
//! Split out of `daemon_client/mod.rs`, which owns the *connection* — finding
//! the daemon, launching it, checking its identity. That is a different
//! concern from the shapes travelling over it, and the file sat on the 300 LOC
//! ratchet with new fields due.

use serde::{Deserialize, Serialize};

use gglib_core::ports::PinnedSpec;

/// Body for `POST /api/proxy/start` — the daemon-side twin of
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
