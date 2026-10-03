//! Which upstream `POST /api/agent/chat` drives: a llama-server this daemon
//! started, or a model of the machine on the other end of the remote tunnel.
//!
//! One decision, taken before anything else in the handler, because the two
//! cases differ in every input the adapter takes: the local case validates
//! the port against the servers this daemon owns and resolves the model
//! against this machine's catalog; the far case takes the port the tunnel
//! bound, attaches the key from the pairing (ADR 0012, decision 7 — the
//! listener does not inject it), looks the model up there by its id, and
//! shapes nothing, because the far proxy runs its own pipeline over its own
//! models.

use gglib_app_services::FarProxy;
use gglib_app_services::types::ServerInfo;
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::{AdmissionLease, ModelRuntimePort};
use gglib_core::request_pipeline::{self, ModelContext};
use gglib_runtime::FarMachine;

use super::AgentChatRequest;
use super::compose::MadeBy;
use crate::handlers::remote::far_error;
use crate::{error::HttpError, handlers::port_utils::validate_port, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1`.
    pub base_url: String,
    /// The far machine on the far path — its key and the name it is
    /// shown by; nothing locally.
    pub far_machine: Option<FarMachine>,
    /// Resolved locally; passthrough for the far machine, whose proxy
    /// resolves.
    pub model_context: ModelContext,
    /// What goes in the body's `model` field.
    ///
    /// Locally `None` is the ordinary case and means "whatever llama-server
    /// loaded". On the far path it is always the model's id there, so the
    /// far proxy serves the model that was picked and no other of its name.
    pub model: Option<String>,
    /// The model name this run's guard decisions are counted under (#1091).
    ///
    /// Never absent, where [`Self::model`] may be: a counter keyed on nothing
    /// is not a counter. Locally, the name the request gave, or — when it gave
    /// none, or gave only whitespace, which is the same absence — the name of
    /// the model actually running on the port it was sent to. On the far
    /// path, the name that machine has for the model the id names.
    ///
    /// This is deliberately not `model.unwrap_or(…)` at the point of use: the
    /// fallback needs the running server, which only this module has, and a
    /// placeholder would file real traffic under something that is not a
    /// model.
    pub counted_as: String,
    /// The model each turn is made by, as its saved row names it: locally
    /// the one loaded on the port with its catalogue quantisation, on the
    /// far path the one the id names there, with that machine's.
    pub made_by: MadeBy,
    /// Locally, the port and the id of the model found on it; otherwise none.
    pub local_model: Option<(u16, i64)>,
    /// On the far path, the model driven, by its machine; otherwise none.
    pub far_model: Option<ModelRef>,
}

/// The model this request named, if it named one.
///
/// A name that is nothing but whitespace is no name: it reaches llama-server
/// as an empty model, and it would key a counter on nothing.
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

/// Resolve the upstream for one request.
///
/// # Errors
///
/// Locally, whatever `validate_port` says. On the far path, `400` for a ref
/// to this machine; `409` when this machine is not connected, holds no key
/// for the machine it is connected to (`RemoteOps::far` refuses both, in
/// words that name the fix) or is connected to another machine than the
/// ref's; and the far machine's refusal of the model, such as a `404` for
/// an id it does not have.
pub(super) async fn resolve(
    state: &AppState,
    req: &AgentChatRequest,
) -> Result<Upstream, HttpError> {
    let Some(far) = &req.far else {
        let server = validate_port(state, req.port).await?;
        return Ok(local(state, req, server).await);
    };
    // Before the connection is read: the request's own shape is settled
    // before this machine's state is, so a ref to this machine is a `400`
    // whether or not a tunnel happens to be up.
    if far.machine == Machine::Local {
        return Err(HttpError::BadRequest(
            "`far` names a model on this machine; a model here is driven by its server's `port`"
                .to_owned(),
        ));
    }
    let proxy = state.remote.far_for(&far.machine).await?;
    remote(&proxy, far).await
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
        far_model: None,
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

/// A far request's upstream: the far proxy's root through the tunnel, the
/// far machine as the adapter carries it, and `model`'s id as the body's
/// model. The id is looked up there first, so the run is counted under,
/// and its turns made by, the name and quantisation that machine has for
/// it, and a model it does not have is refused before any turn starts.
///
/// # Errors
///
/// The far machine's refusal of the lookup, with its status and message.
pub(super) async fn remote(far: &FarProxy, model: &ModelRef) -> Result<Upstream, HttpError> {
    let id = model.id.to_string();
    let detail = far.lookup(&id).await.map_err(far_error)?.detail;
    Ok(Upstream {
        base_url: far.server_root(),
        // The name travels with the key because only the `FarProxy` this was
        // built from holds both: the request that fails on a rotated key
        // comes back to the adapter, which by then has no way to ask who was
        // asked.
        far_machine: Some(far.far_machine()),
        model_context: ModelContext::passthrough(),
        // The far machine counts its own guard decisions under this name, in
        // its own ledger; this one counts what it composed here.
        counted_as: detail.name.clone(),
        made_by: MadeBy {
            model: detail.name,
            quantization: detail.quantization,
            device: None,
        },
        model: Some(id),
        local_model: None,
        far_model: Some(model.clone()),
    })
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
