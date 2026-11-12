//! Which upstream an agent loop drives: this machine's endpoint, for a turn
//! on one of this machine's models ([`through_endpoint`]), or a model of the
//! machine on the other end of the remote tunnel ([`far`]).
//!
//! The two differ in every input the adapter takes: through this machine's
//! endpoint the turn names the model by its catalogue id, reads its row
//! here, and carries the endpoint's own layers for the one thing the turn
//! still folds; the far case takes the port the tunnel bound, attaches the
//! key from the pairing (ADR 0012, decision 7 — the listener does not
//! inject it), looks the model up there by its id, and shapes nothing,
//! because the far proxy runs its own pipeline over its own models.

use gglib_app_services::FarProxy;
use gglib_app_services::GuiError;
use gglib_app_services::transcript::MadeBy;
use gglib_core::Settings;
use gglib_core::domain::{InferenceProfile, Machine, ModelRef};
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_core::ports::ModelRuntimePort;
use gglib_core::request_pipeline::{self, ModelContext, SamplingLayers};
use gglib_runtime::Door;
use gglib_runtime::proxy::ProxyStatus;

use super::chat_sampling::ChatSampling;
use super::hub_model::Loaded;
use super::loop_request::LoopRequest;
use crate::handlers::remote::far_error;
use crate::{error::HttpError, state::AppState};

/// Where the completion adapter points, and with what.
pub(super) struct Upstream {
    /// The root the loop posts to, without the `/v1`: the door's.
    pub base_url: String,
    /// The endpoint the turn goes through: this machine's ([`endpoint`]), or
    /// the far machine's on the far path, with its key and the name it is
    /// shown by.
    pub door: Door,
    /// The context of the model the turn runs on, by its catalogue row
    /// here; passthrough for the far machine, whose proxy resolves, but for
    /// whether it reads images.
    pub model_context: ModelContext,
    /// The stored sampling layers the endpoint folds beneath what the turn
    /// names ([`endpoint_layers`]). For the far machine, whose proxy folds
    /// its own, no layer.
    pub layers: SamplingLayers,
    /// What goes in the body's `model` field: the model's id at the
    /// endpoint, so the endpoint serves the model that was picked and no
    /// other of its name, with the chat's profile as its `:<profile>`
    /// suffix.
    pub model: Option<String>,
    /// The model name this run's guard decisions are counted under (#1091):
    /// the name the catalogue, or the far machine, has for the model.
    pub counted_as: String,
    /// The model each turn is made by, as its saved row names it: the
    /// catalogue's name and quantisation for the id, or on the far path the
    /// one the id names there, with that machine's.
    pub made_by: MadeBy,
    /// Locally, the port and the id of the model found on it; otherwise none.
    pub local_model: Option<(u16, i64)>,
    /// On the far path, the model driven, by its machine; otherwise none.
    pub far_model: Option<ModelRef>,
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
/// on first when it is off, as starting a model turns it on, and said to be
/// on (`proxy_started`) so every window showing it Off hears it.
///
/// # Errors
///
/// `conflict` (409) with the endpoint's own reason when it cannot be turned
/// on (its port held by another process); `invalid_request` (400) naming
/// that it is off, should it stop between the two; `internal_error` (500)
/// when the settings that hold its key cannot be read.
pub(super) async fn this_machines_door(state: &AppState) -> Result<Door, HttpError> {
    let was_off = !matches!(state.proxy.status().await, ProxyStatus::Running { .. });
    let address = state.proxy.ensure_running().await?;
    if was_off {
        state.sse.emit(AppEvent::proxy_started(address.port()));
    }
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
    req: &LoopRequest,
    loaded: Loaded,
    door: Door,
) -> Result<Upstream, HttpError> {
    let Loaded { id, port } = loaded;
    let by_id = id.to_string();
    super::image_gate::chats_named(state, &by_id).await?;
    super::image_gate::served(state, id, &req.messages).await?;
    let model_context = request_pipeline::resolve(state.catalog.as_ref(), Some(&by_id)).await;
    let settings = state.core.settings().get().await.unwrap_or_default();
    let profile = req.saved.configured_profile(&settings);
    let layers = endpoint_layers(state, &settings, profile).await;
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
        door,
        model_context,
        layers,
        // The chat's saved profile rides on the id, which the endpoint
        // routes to that profile whether or not it trusts clients' sampling;
        // one no longer configured is left off, and the turn runs without it.
        model: Some(profile.map_or_else(|| by_id.clone(), |p| format!("{by_id}:{}", p.name))),
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

/// The stored layers the running endpoint folds the turn's request with:
/// the sampling it was started with (`gglib serve`'s flags), a profile, this
/// machine's global defaults. The profile is the
/// chat's, `chat`, when the turn names one, as the endpoint routes
/// `<id>:<profile>` to it, and otherwise the endpoint's default. Handed to
/// the adapter for the one thing it folds through a door: whether the
/// endpoint's ladder names a `max_tokens`, which a turn's limit would
/// otherwise outrank. A settings read that fails leaves the global layer and
/// the default profile out.
async fn endpoint_layers(
    state: &AppState,
    settings: &Settings,
    chat: Option<&InferenceProfile>,
) -> SamplingLayers {
    let (cli_override, default_profile) = state.proxy.started_sampling().await.unwrap_or_default();
    // As the endpoint reads it: a default profile that is no longer
    // configured is no layer, and a profile the request names replaces it.
    let profile = chat.map(|p| p.config.clone()).or_else(|| {
        let name = default_profile?;
        let profiles = settings.inference_profiles.as_deref().unwrap_or_default();
        let found = profiles.iter().find(|p| p.name == name)?;
        Some(found.config.clone())
    });
    SamplingLayers {
        cli_override,
        profile,
        global: settings.inference_defaults.clone(),
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
    req: &LoopRequest,
    loaded: Loaded,
) -> Result<Upstream, HttpError> {
    let door = this_machines_door(state).await?;
    endpoint(state, req, loaded, door).await
}

/// The upstream of a turn on `model`, a model of the paired machine, with
/// what its chat `saved`: what [`remote`] makes of the far proxy `reach`
/// finds for that machine (`RemoteOps::far_for`, in the daemon).
///
/// # Errors
///
/// `409` when this machine is not connected, holds no key for the machine
/// it is connected to (`RemoteOps::far` refuses both, in words that name
/// the fix) or is connected to another machine than the ref's; and the far
/// machine's refusal of the model, such as a `404` for an id it does not
/// have.
pub(super) async fn far(
    reach: impl AsyncFnOnce(&Machine) -> Result<FarProxy, GuiError>,
    model: &ModelRef,
    saved: &ChatSampling,
) -> Result<Upstream, HttpError> {
    let proxy = reach(&model.machine).await?;
    remote(&proxy, model, saved).await
}

/// A far request's upstream: the far proxy's root through the tunnel, the
/// far machine as the adapter carries it, and `model`'s id as the body's
/// model, with the profile the chat `saved` as its `{id}:{profile}` suffix,
/// which that machine routes to a profile of its own. The id is looked up
/// there first, so the run is counted under,
/// and its turns made by, the name and quantisation that machine has for
/// it, and a model it does not have is refused before any turn starts.
///
/// # Errors
///
/// The far machine's refusal of the lookup, with its status and message.
pub(super) async fn remote(
    far: &FarProxy,
    model: &ModelRef,
    saved: &ChatSampling,
) -> Result<Upstream, HttpError> {
    let id = model.id.to_string();
    let detail = far.lookup(&id).await.map_err(far_error)?.detail;
    Ok(Upstream {
        base_url: far.server_root(),
        // The name travels with the key because only the `FarProxy` this was
        // built from holds both: the request that fails on a rotated key
        // comes back to the adapter, which by then has no way to ask who was
        // asked.
        door: far.door(),
        // Nothing of that machine's row is known here but what its lookup
        // answered. Whether the model reads images is, and decides one
        // thing: whether it is shown the most recent drawn picture.
        model_context: ModelContext {
            image_input: detail.image_input,
            ..ModelContext::passthrough()
        },
        // Nothing stored here applies to that machine's model.
        layers: SamplingLayers::default(),
        // The far machine counts its own guard decisions under this name, in
        // its own ledger; this one counts what it composed here.
        counted_as: detail.name.clone(),
        made_by: MadeBy {
            model: detail.name,
            quantization: detail.quantization,
            device: None,
            context_size: None,
        },
        model: Some(saved.model(id)),
        local_model: None,
        far_model: Some(model.clone()),
    })
}

#[cfg(test)]
#[path = "remote_upstream_endpoint_tests.rs"]
mod endpoint_tests;
#[cfg(test)]
#[path = "remote_upstream_tests.rs"]
mod tests;
