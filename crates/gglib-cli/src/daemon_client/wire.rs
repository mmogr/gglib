//! Request and response bodies for the daemon's HTTP API.
//!
//! One half of a two-sided contract: each type here must serialise compatibly
//! with what the daemon sends or expects, or the CLI silently drops a field.
//! `StartProxyBody` and `ProxyStatusDto` pair with
//! `gglib_axum::handlers::proxy::wire`, and carry the same filename so the
//! pairing is visible from the tree; `StartServerBody` pairs with its namesake
//! in `gglib_axum::handlers::servers`, and carries the daemon's own
//! `StartServerRequest`; `StartServerDto` narrows
//! `gglib_app_services::types::StartServerResponse`, `QueueDownloadBody`
//! pairs with `gglib_axum::handlers::model::downloads`, and `RepairBody` with
//! `RepairRequest` in `gglib_axum::handlers::model::verification`. The tests
//! in `wire_tests.rs` pin `StartProxyBody`, `StartServerBody` and
//! `StartServerDto`, the test of the queue request in `calls.rs` reads
//! `QueueDownloadBody` as it was sent, and `repair_tests.rs` reads
//! `RepairBody` so. `ProxyStatusDto` is read against the status's own key
//! list, `PROXY_STATUS_FIELDS`, which the daemon's test holds its struct to.
//! The answers to a queue
//! request and to a repair have no twin here: the CLI reads the daemon's own
//! `gglib_app_services::types::QueueDownloadResponse` and
//! `gglib_core::services::RepairStarted`.
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

use gglib_app_services::types::StartServerRequest;
use gglib_core::domain::KeyState;
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
    /// Required: a daemon that does not say it is older than this CLI, and the
    /// build-mismatch warning says so; a guessed word would be a wrong one.
    pub state: gglib_core::domain::EndpointState,
    pub port: Option<u16>,
    /// Where a client reaches it, `http://<address>/v1`; `None` while not
    /// listening.
    pub base_url: Option<String>,
    /// Whether a request must carry the API key; `None` while not listening,
    /// and while listening when the daemon could not read its settings to
    /// say.
    pub key: Option<KeyState>,
    #[serde(default)]
    pub pinned_model: Option<String>,
}

/// One copyable line of a client recipe, as the daemon sends it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RecipeLineDto {
    pub label: String,
    pub value: String,
}

/// One client's recipe, as the daemon sends it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClientRecipeDto {
    pub client: String,
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub lines: Vec<RecipeLineDto>,
}

/// `GET /api/proxy/clients?model=` response: how a client connects to the
/// endpoint for one model, the daemon's facts and its recipes. Mirror of
/// `gglib_app_services::ClientRecipes`, held to `PROXY_CLIENTS_FIELDS`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClientRecipesDto {
    pub base_url: String,
    /// `None` when the daemon could not read its settings to say.
    pub key: Option<KeyState>,
    pub model_id: String,
    pub gglib_id: i64,
    #[serde(default)]
    pub recipes: Vec<ClientRecipeDto>,
}

/// `POST /api/proxy/prove` response: the endpoint answered. Mirror of
/// `gglib_app_services::ProofOutcome`, held to `PROXY_PROVE_FIELDS`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProofOutcomeDto {
    pub answered_by: String,
    pub asked_for: String,
    pub elapsed_ms: u64,
}

/// Body for `POST /api/servers/start` — the client-side twin of
/// `gglib_axum::handlers::servers::StartServerBody`.
///
/// The request is the daemon's own type, flat beside the model's id as the
/// daemon reads it, so in one build its fields travel under the names the
/// daemon reads them by. `id` is this struct's own key, and `wire_tests.rs`
/// holds it to the list the daemon's test reads.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StartServerBody {
    pub id: i64,
    #[serde(flatten)]
    pub config: StartServerRequest,
}

/// `POST /api/servers/start` response: the port, and the daemon's own
/// sentence about the start, which `gglib model load` prints as it came.
///
/// The CLI's reading of `gglib_app_services::types::StartServerResponse`;
/// `a_start_server_response_deserializes_into_the_narrowing` pins it.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StartServerDto {
    pub port: u16,
    pub message: String,
}

/// Body for `POST /api/servers/stop` — the client-side twin of
/// `gglib_axum::handlers::servers::StopServerBody`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StopServerBody {
    pub model_id: i64,
}

/// Body for `POST /api/proxy/start-pinned` — the client-side twin of
/// `gglib_axum::handlers::proxy::StartPinnedBody`: the app's pinned start,
/// which the daemon runs `gglib serve`'s launch cascade for.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StartPinnedBody {
    pub model_id: i64,
    /// The model's launch options, in the start body's `camelCase`.
    pub options: StartServerRequest,
    /// The proxy's own start config, in the proxy body's `snake_case`.
    pub proxy: StartProxyBody,
}

/// `POST /api/models/downloads/queue` request body.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct QueueDownloadBody {
    pub model_id: String,
    /// `None` leaves the quantization choice to the daemon.
    pub quant: Option<String>,
}

/// `POST /api/models/{id}/repair` request body.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepairBody {
    /// The file indices to repair. `None` is every unhealthy file.
    pub shards: Option<Vec<usize>>,
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
