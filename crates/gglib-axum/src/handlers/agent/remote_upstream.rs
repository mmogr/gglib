//! Which upstream `POST /api/agent/chat` drives: a llama-server this daemon
//! started, or the machine on the other end of the remote tunnel.
//!
//! One decision, taken before anything else in the handler, because the two
//! cases differ in every input the adapter takes: the local case validates
//! the port against the servers this daemon owns and resolves the model
//! against this machine's catalog; the remote case takes the port the tunnel
//! bound, attaches the key from the pairing (ADR 0012, decision 7 — the
//! listener does not inject it), and shapes nothing, because the far proxy
//! runs its own pipeline over its own models.

use gglib_app_services::types::ServerInfo;
use gglib_core::ports::{AdmissionLease, ModelRuntimePort};
use gglib_core::request_pipeline::{self, ModelContext};
use gglib_runtime::FarMachine;

use super::AgentChatRequest;
use super::compose::MadeBy;
use crate::{error::HttpError, handlers::port_utils::validate_port, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1`.
    pub base_url: String,
    /// The far machine on the remote path — its key and the fingerprint it
    /// is known by; nothing locally.
    pub far_machine: Option<FarMachine>,
    /// Resolved locally; passthrough for the remote, whose proxy resolves.
    pub model_context: ModelContext,
    /// The name that goes in the body's `model` field.
    ///
    /// Locally `None` is the ordinary case and means "whatever llama-server
    /// loaded". Remotely it is always `Some`, because [`remote_model`] has
    /// already refused the request that named nothing — the two cases read
    /// the same field and mean opposite things by an absence, so the
    /// decision is taken here rather than left for the handler.
    pub model: Option<String>,
    /// The model name this run's guard decisions are counted under (#1091).
    ///
    /// Never absent, where [`Self::model`] may be: a counter keyed on nothing
    /// is not a counter. Locally, the name the request gave, or — when it gave
    /// none, or gave only whitespace, which is the same absence — the name of
    /// the model actually running on the port it was sent to. Remotely it is
    /// the name the request gave, which [`remote_model`] has already refused
    /// to let be absent.
    ///
    /// This is deliberately not `model.unwrap_or(…)` at the point of use: the
    /// fallback needs the running server, which only this module has, and a
    /// placeholder would file real traffic under something that is not a
    /// model.
    pub counted_as: String,
    /// The model each turn is made by, as its saved row names it: locally
    /// the one loaded on the port with its catalogue quantisation, remotely
    /// the one named, with none (this machine has no catalogue for it).
    pub made_by: MadeBy,
    /// Locally, the port and the id of the model found on it; remotely none.
    pub local_model: Option<(u16, i64)>,
}

/// The model this request named, if it named one.
///
/// A name that is nothing but whitespace is no name: it reaches llama-server
/// as an empty model, and it would key a counter on nothing. Both paths read
/// an absent model through this one function, so what counts as absent cannot
/// come to differ between them.
fn named_model(req: &AgentChatRequest) -> Option<String> {
    req.model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(ToOwned::to_owned)
}

/// The name a local run's guard decisions are counted under.
///
/// The request's own name when it gave one, and otherwise the model actually
/// running on the port it was sent to. Locally an absent model is the ordinary
/// case — it means "whatever llama-server loaded" — so recording nothing would
/// blind the instrument exactly where most of the traffic is, and a
/// placeholder would file real traffic under something that is not a model.
///
/// Its own function because `resolve` cannot be driven in a test: the axum
/// harness bootstraps an `AppState` with no running servers, so `validate_port`
/// refuses before any of this is reached.
fn counted_as(req: &AgentChatRequest, server: &ServerInfo) -> String {
    named_model(req).unwrap_or_else(|| server.model_name.clone())
}

/// The far machine's model name, or the refusal that says why there is none.
///
/// Only the remote path asks: locally an absent model is legitimate and means
/// something else entirely.
///
/// # Errors
///
/// `400` when the body named no model, or named one that is nothing but
/// whitespace — the same absence, and the same empty model downstream.
fn remote_model(req: &AgentChatRequest) -> Result<String, HttpError> {
    named_model(req).ok_or_else(|| {
        HttpError::BadRequest(
            "no model named for the remote machine — name one it serves (`gglib model list` \
                 there is the list). This machine's default is not sent, because that machine may \
                 not have it"
                .to_owned(),
        )
    })
}

/// Resolve the upstream for one request.
///
/// # Errors
///
/// Locally, whatever `validate_port` says. Remotely, `409` when this machine
/// is not connected, or is connected but holds no key — both are things
/// `gglib remote join` fixes, and the message says so — and `400` when the
/// body named no model, which nothing downstream can fix.
pub(super) async fn resolve(
    state: &AppState,
    req: &AgentChatRequest,
) -> Result<Upstream, HttpError> {
    if !req.remote {
        let server = validate_port(state, req.port).await?;
        return Ok(local(state, req, server).await);
    }

    // Before the connection is read: the request's own shape is settled
    // before this machine's state is, so a body that names no model is a
    // `400` whether or not a tunnel happens to be up.
    let model = remote_model(req)?;

    let Some(connection) = state.remote.status().await.connected else {
        return Err(HttpError::Conflict(
            "not connected to a remote machine — `gglib remote join` first".to_owned(),
        ));
    };
    // The core settings, not the GUI's `AppSettings`: the key is deliberately
    // absent from the shapes the settings panel reads.
    let key = state
        .core
        .settings()
        .get()
        .await
        .map_err(|e| HttpError::Internal(format!("could not read settings: {e}")))?
        .remote_pairing
        .map(|stored| stored.api_key)
        .ok_or_else(|| {
            HttpError::Conflict(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string"
                    .to_owned(),
            )
        })?;
    Ok(remote(
        model,
        connection.port,
        connection.ticket_fingerprint,
        key,
    ))
}

/// A local request's upstream, once its port is known to serve `server`.
pub(super) async fn local(
    state: &AppState,
    req: &AgentChatRequest,
    server: ServerInfo,
) -> Upstream {
    let model_context =
        request_pipeline::resolve(state.catalog.as_ref(), req.model.as_deref()).await;
    Upstream {
        base_url: format!("http://127.0.0.1:{}", req.port),
        far_machine: None,
        model_context,
        model: req.model.clone(),
        counted_as: counted_as(req, &server),
        made_by: MadeBy {
            quantization: quantization_of(state, server.model_id).await,
            model: server.model_name,
            device: None,
        },
        local_model: Some((req.port, server.model_id)),
    }
}

/// A local run's hold on the model it resolved, which its loop talks to past
/// the proxy's queue: while held, no proxy request swaps or recycles it. A
/// remote run holds nothing here; the far proxy admits each of its requests.
///
/// # Errors
///
/// `unavailable` (503) when that model is no longer the one on its port: a
/// swap came between resolving it and holding it.
pub(super) fn hold(
    runtime: &dyn ModelRuntimePort,
    local_model: Option<(u16, i64)>,
) -> Result<Option<AdmissionLease>, HttpError> {
    let Some((port, model_id)) = local_model else {
        return Ok(None);
    };
    let held = u32::try_from(model_id)
        .ok()
        .and_then(|id| runtime.hold(port, id));
    held.map(Some).ok_or_else(|| HttpError::Coded {
        status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
        code: "unavailable",
        message: format!(
            "the model on port {port} was stopped or swapped while the run was prepared; \
             try again"
        ),
    })
}

/// A remote request's upstream: the tunnel's `port`, the far machine's key
/// and the fingerprint it is known by, and the `model` named there.
pub(super) fn remote(model: String, port: u16, fingerprint: String, key: String) -> Upstream {
    Upstream {
        base_url: format!("http://127.0.0.1:{port}"),
        // The fingerprint travels with the key because only this function
        // knows both: the request that fails on a rotated key comes back to
        // the adapter, which by then has no way to ask who was asked.
        far_machine: Some(FarMachine { key, fingerprint }),
        model_context: ModelContext::passthrough(),
        // The far machine counts its own guard decisions under this name, in
        // its own ledger; this one counts what it composed here.
        counted_as: model.clone(),
        made_by: MadeBy {
            model: model.clone(),
            quantization: None,
            device: None,
        },
        model: Some(model),
        local_model: None,
    }
}

/// The catalogue's quantisation for a served model; `None` when it has none
/// or cannot be read, which leaves the figure out rather than failing a run.
async fn quantization_of(state: &AppState, model_id: i64) -> Option<String> {
    let model = state
        .core
        .models()
        .get_by_id(model_id)
        .await
        .ok()
        .flatten()?;
    model.quantization
}

#[cfg(test)]
#[path = "remote_upstream_tests.rs"]
mod tests;
