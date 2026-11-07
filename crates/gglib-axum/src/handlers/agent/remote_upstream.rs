//! Which upstream an agent loop drives: this machine's endpoint, for a turn
//! on one of this machine's chats or a device's ([`through_endpoint`]); a
//! llama-server this daemon started, for `POST /api/agent/chat`; or a model of
//! the machine on the other end of the remote tunnel.
//!
//! One decision, taken before anything else in the handler, because the two
//! cases differ in every input the adapter takes: the local case validates
//! the port against the servers this daemon owns, shapes the turn for the
//! model that port serves, by its row in this machine's catalog, and hands
//! over this machine's global sampling defaults and its agentic sampling
//! switch; the far case takes the port the tunnel bound, attaches the key
//! from the pairing (ADR 0012, decision 7 — the listener does not inject
//! it), looks the model up there by its id, and shapes nothing, because the
//! far proxy runs its own pipeline over its own models.

use gglib_app_services::FarProxy;
use gglib_app_services::GuiError;
use gglib_app_services::transcript::MadeBy;
use gglib_app_services::types::ServerInfo;
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::ModelRuntimePort;
use gglib_core::request_pipeline::{self, ModelContext, SamplingLayers};
use gglib_runtime::Door;

use super::AgentChatRequest;
use super::hub_model::Loaded;
use crate::handlers::remote::far_error;
use crate::{error::HttpError, handlers::port_utils::validate_port, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// The root the loop posts to, without the `/v1`: the door's, or
    /// `http://127.0.0.1:<port>` for a llama-server.
    pub base_url: String,
    /// The endpoint the turn goes through: this machine's ([`endpoint`]), or
    /// the far machine's on the far path, with its key and the name it is
    /// shown by; none for a llama-server.
    pub door: Option<Door>,
    /// Locally, the context of the model the port serves; passthrough for
    /// the far machine, whose proxy resolves.
    pub model_context: ModelContext,
    /// The stored sampling layers beneath what the request names. Locally,
    /// this machine's global defaults and its agentic sampling switch, and
    /// no profile: a run's request can name none. For the far machine, whose
    /// proxy folds its own, no layer, and the ceiling on.
    pub layers: SamplingLayers,
    /// What goes in the body's `model` field.
    ///
    /// On a llama-server's port `None` is the ordinary case and means
    /// "whatever llama-server loaded". Through an endpoint it is always the
    /// model's id there, so the endpoint serves the model that was picked
    /// and no other of its name.
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
/// Locally, whatever `validate_port` says, `image_model_cannot_chat` (400)
/// when the port serves a model that draws, and `model_cannot_read_images`
/// (400) when a message carries an image the served model cannot read. On
/// the far path, where that is the far proxy's to refuse, `400` for a ref
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
        return local(state, req, server).await;
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
///
/// # Errors
///
/// `image_model_cannot_chat` (400) when `server` is an `sd-server`, which
/// draws and cannot chat; `model_cannot_read_images` (400) when a message,
/// history included, carries an image and the model `server` serves has no
/// projector.
pub(super) async fn local(
    state: &AppState,
    req: &AgentChatRequest,
    server: ServerInfo,
) -> Result<Upstream, HttpError> {
    super::image_gate::chats(server.runtime, &server.model_name)?;
    super::image_gate::served(state, server.model_id, &req.messages).await?;
    // By its id: the catalog row the server was started from. Never by the
    // request's `model`, which the page and a paired device leave empty and
    // which names only what llama-server is to route by.
    let by_id = server.model_id.to_string();
    let model_context = request_pipeline::resolve(state.catalog.as_ref(), Some(&by_id)).await;
    // A settings read that fails leaves the global layer out and the agentic
    // switch on, as it leaves the limits at their defaults
    // (`compose::config_for`).
    let settings = state.core.settings().get().await.unwrap_or_default();
    let layers = SamplingLayers {
        agentic_adjustments: settings.effective_agentic_sampling(),
        global: settings.inference_defaults,
        ..SamplingLayers::default()
    };
    Ok(Upstream {
        base_url: format!("http://127.0.0.1:{}", req.port),
        door: None,
        model_context,
        layers,
        model: req.model.clone(),
        counted_as: counted_as(req, &server),
        made_by: MadeBy {
            quantization: quantization_of(state, server.model_id).await,
            model: server.model_name,
            device: None,
            context_size: None,
        },
        local_model: Some((req.port, server.model_id)),
        far_model: None,
    })
}

/// The context the model a local run drives was launched with: the primary
/// slot's, when the model in it is the run's own, by its port and its id.
///
/// `None` for a paired machine's model, for a model in the second slot, and
/// when another model is the primary one. Never a default: a size the run
/// does not know is a figure its reply leaves out.
async fn launched_context(
    runtime: &dyn ModelRuntimePort,
    local_model: Option<(u16, i64)>,
) -> Option<u64> {
    let (port, model_id) = local_model?;
    let running = runtime.current_model().await?;
    let same = running.port == port && i64::from(running.model_id) == model_id;
    same.then_some(running.effective_ctx)
}

/// This machine's endpoint, as a turn on one of its models dials it: turned
/// on first when it is off, as starting a model turns it on.
///
/// # Errors
///
/// `conflict` (409) with the endpoint's own reason when it cannot be turned
/// on (its port held by another process); `invalid_request` (400) naming
/// that it is off, should it stop between the two; `internal_error` (500)
/// when the settings that hold its key cannot be read.
pub(super) async fn this_machines_door(state: &AppState) -> Result<Door, HttpError> {
    state.proxy.ensure_running().await?;
    state
        .proxy
        .door()
        .await
        .map_err(|closed| HttpError::from(GuiError::from(closed)))
}

/// A turn on this machine's model `loaded`, through this machine's endpoint
/// `door`, as any client sends one: to the door's root with its key, naming
/// the model by its catalogue id, which the endpoint resolves before any
/// name and loads again if it is no longer resident.
///
/// Everything is read from the catalogue's row for that id, never from what
/// its port serves now: the load's lease is gone, so another client may have
/// swapped the model out, or another in onto the same port, and the turn is
/// still the chat's model's. The endpoint shapes the request; the model's
/// context and the endpoint's own layers ([`endpoint_layers`]) are kept for
/// what the turn still reads of them here: whether it sees images, and
/// whether the endpoint's ladder names a `max_tokens`, for the turn's limit. The turns report the context the model was launched
/// with when it is still the one in the primary slot (#1272), and none
/// otherwise. The endpoint trims to the context it serves.
///
/// No hold is taken: the endpoint admits each request, and its lease is the
/// turn's.
///
/// # Errors
///
/// `image_model_cannot_chat` (400) for a model that draws, and
/// `model_cannot_read_images` (400) when a message carries an image it
/// cannot read.
pub(super) async fn endpoint(
    state: &AppState,
    req: &AgentChatRequest,
    loaded: Loaded,
    door: Door,
) -> Result<Upstream, HttpError> {
    let Loaded { id, port } = loaded;
    let by_id = id.to_string();
    super::image_gate::chats_named(state, &by_id).await?;
    super::image_gate::served(state, id, &req.messages).await?;
    let model_context = request_pipeline::resolve(state.catalog.as_ref(), Some(&by_id)).await;
    let layers = endpoint_layers(state).await;
    let row = state.core.models().get_by_id(id).await.ok().flatten();
    let name = row
        .as_ref()
        .map_or_else(|| by_id.clone(), |m| m.name.clone());
    let quantization = row
        .as_ref()
        .and_then(|m| m.quant_label().map(str::to_owned));
    let local_model = Some((port, id));
    Ok(Upstream {
        base_url: door.root_url.clone(),
        door: Some(door),
        model_context,
        layers,
        model: Some(by_id),
        counted_as: name.clone(),
        made_by: MadeBy {
            model: name,
            quantization,
            device: None,
            context_size: launched_context(state.runtime.as_ref(), local_model).await,
        },
        local_model,
        far_model: None,
    })
}

/// The stored layers the running endpoint folds a bare id's request with:
/// the sampling it was started with (`gglib serve`'s flags), its default
/// profile, this machine's global defaults and its agentic switch. Handed to
/// the adapter for the one thing it folds through a door: whether the
/// endpoint's ladder names a `max_tokens`, which a turn's limit would
/// otherwise outrank. A settings read that fails leaves the global layer and
/// the profile out, and the switch on.
async fn endpoint_layers(state: &AppState) -> SamplingLayers {
    let settings = state.core.settings().get().await.unwrap_or_default();
    let (cli_override, default_profile) = state.proxy.started_sampling().await.unwrap_or_default();
    // As the endpoint reads it: a default profile that is no longer
    // configured is no layer.
    let profile = default_profile.and_then(|name| {
        let profiles = settings.inference_profiles.as_deref().unwrap_or_default();
        let found = profiles.iter().find(|p| p.name == name)?;
        Some(found.config.clone())
    });
    SamplingLayers {
        cli_override,
        profile,
        agentic_adjustments: settings.effective_agentic_sampling(),
        global: settings.inference_defaults,
        ..SamplingLayers::default()
    }
}

/// [`endpoint`] for a turn whose model was just found running or loaded,
/// once [`this_machines_door`] is open.
///
/// # Errors
///
/// [`this_machines_door`]'s, then [`endpoint`]'s.
pub(super) async fn through_endpoint(
    state: &AppState,
    req: &AgentChatRequest,
    loaded: Loaded,
) -> Result<Upstream, HttpError> {
    let door = this_machines_door(state).await?;
    endpoint(state, req, loaded, door).await
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
        door: Some(far.door()),
        // Nothing of that machine's row is known here but what its lookup
        // answered. Whether the model reads images is, and decides one
        // thing: whether it is shown the most recent drawn picture.
        model_context: ModelContext {
            image_input: detail.image_input,
            ..ModelContext::passthrough()
        },
        // Nothing stored here applies to that machine's model, this
        // machine's agentic switch included: the ceiling stays on.
        layers: SamplingLayers {
            agentic_adjustments: true,
            ..SamplingLayers::default()
        },
        // The far machine counts its own guard decisions under this name, in
        // its own ledger; this one counts what it composed here.
        counted_as: detail.name.clone(),
        made_by: MadeBy {
            model: detail.name,
            quantization: detail.quantization,
            device: None,
            context_size: None,
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
    model.quant_label().map(str::to_owned)
}

#[cfg(test)]
#[path = "remote_upstream_endpoint_tests.rs"]
mod endpoint_tests;
#[cfg(test)]
#[path = "remote_upstream_tests.rs"]
mod tests;
