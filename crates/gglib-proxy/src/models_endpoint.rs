//! What `/v1/models` advertises, and why that number has to be right.
//!
//! Clients like the GitHub Copilot LLM Gateway extension read this endpoint
//! **once** when building their model picker, typically before any model is
//! running, and budget against the answer for the whole session. So the
//! pre-launch advertisement has to describe what a launch would actually
//! serve, not a number that is merely available to it.
//!
//! Split out of `server.rs` because everything deciding that number belongs
//! together: the safety margin, the running model's live override, and the
//! cap for a host that cannot fit a context at all.
//!
//! `GET /v1/models/{name}` answers one entry of that same list.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::{debug, error};

use crate::models::{ErrorResponse, Feature, GglibInfo, ModelInfo, ModelsResponse};
use crate::profiles::variant_entries;
use crate::refusal::Segment;
use crate::server::AppState;

/// Percentage shaved off a model's raw context window when advertised via
/// `/v1/models`.
///
/// Reserves headroom for the tool-schema JSON and chat-template tokens that a
/// client's own char→token budget estimate (e.g. the VS Code LLM Gateway's
/// `CHARS_PER_TOKEN = 4`) does not account for. Advertising slightly less than
/// the true ceiling makes such clients begin proactive context compaction
/// before the real limit is hit, avoiding upstream context-overflow rejections
/// on the final turns of a long session.
const CONTEXT_WINDOW_SAFETY_MARGIN_PCT: u64 = 8;

/// Apply [`CONTEXT_WINDOW_SAFETY_MARGIN_PCT`] to a raw context-window token
/// count, returning the value to advertise to clients.
fn advertised_context_window(raw_ctx: u64) -> u64 {
    raw_ctx.saturating_mul(100 - CONTEXT_WINDOW_SAFETY_MARGIN_PCT) / 100
}

/// List all models from the catalog in `OpenAI` format.
///
/// Every model advertises the context it would actually be served with —
/// clients like the GitHub Copilot LLM Gateway extension read this endpoint
/// ONCE when building their model picker (typically before any model is
/// running), so the pre-launch advertisement must already reflect the real
/// serving context or clients budget against a stale floor for the entire
/// session:
///
/// * **Non-running models**: the GGUF's `context_length`, capped only by a
///   configured per-model or global default
///   — with nothing configured the launch fits the context to this machine,
///   so the trained window is advertised as the upper bound rather than a
///   floor nobody chose — meaning `admit` may launch the model at less than
///   the advertised figure, never more.
/// * **The currently running model**: its full live `effective_ctx` (the
///   real `--ctx-size` llama-server was launched with), which also drives
///   the per-request truncation budget in
///   [`crate::forward::forward_chat_completion`] — advertised and enforced
///   values stay in lockstep.
///
/// Both are shaved by [`CONTEXT_WINDOW_SAFETY_MARGIN_PCT`] before being
/// advertised, reserving headroom for tool-schema JSON and chat-template
/// tokens that a client's own char→token budget does not account for.
///
/// The list also names this machine (`machine_name`), for a paired machine
/// that shows these models beside its own, when the serve's
/// [`HostFacts`](crate::HostFacts) can read the name; otherwise it leaves the
/// field out. And it says which gglib this is (`gglib`): the build's version,
/// and the features this process serves.
pub(crate) async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");
    match entries(&state).await {
        Ok(data) => Json(ModelsResponse {
            object: "list".to_owned(),
            data,
            // Read per request, so a rename shows on the next listing. Here
            // and never on `/health`, which answers before any credential
            // does.
            machine_name: state.host.read_machine_name(),
            gglib: Some(served(&state)),
        })
        .into_response(),
        Err(response) => response,
    }
}

/// `GET /v1/models/{name}`: the entry [`list_models`] lists under `name` as
/// its `id`, exactly as listed.
///
/// Picked from the same list, so the two never disagree: an id the list does
/// not hold is 404 `model_not_found`, even where a chat request would resolve
/// it (a catalogue id, a profile not listed).
pub(crate) async fn retrieve_model(
    State(state): State<AppState>,
    Segment(name): Segment,
) -> Response {
    debug!(model = %name, "GET /v1/models/:name");
    let data = match entries(&state).await {
        Ok(data) => data,
        Err(response) => return response,
    };
    let Some(model) = data.into_iter().find(|m| m.id == name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::model_not_found(&name)),
        )
            .into_response();
    };
    Json(model).into_response()
}

/// Every entry `/v1/models` lists, as it lists them; a catalogue that cannot
/// be read is the 500 to answer with.
async fn entries(state: &AppState) -> Result<Vec<ModelInfo>, Response> {
    let models = state.catalog_port.list_models().await.map_err(|e| {
        error!("Failed to list models: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::internal_error(&format!(
                "Failed to list models: {e}"
            ))),
        )
            .into_response()
    })?;

    // Over the whole catalogue, pinned or not: an entry's `id` is its name
    // only when no other model has that name, and a pinned model listed alone
    // under a name its twin shares would be asked for by a name that resolves
    // to the twin.
    let mut data =
        ModelsResponse::from_summaries(models, state.default_ctx, state.device_memory_readable)
            .data;

    // Pinned mode refuses every other model, so advertising the rest of the
    // catalog would offer a BYOK client a choice that can only come back as
    // PinnedModelMismatch. Filtered before the variants below, which are
    // built from what survives.
    //
    // Profile variants of the pinned model stay: a profile changes only the
    // request body, never which model actually runs, so it cannot trip the
    // guard. Kept by id, as the guard compares.
    if let Some(pin) = state.runtime_port.pinned() {
        data.retain(|m| m.gglib_id == pin.id);
    }

    // The running model's entry, found by id: a name can belong to more than
    // one model.
    let running = state.runtime_port.current_model().await;

    // Apply safety margin to every model's context_window.
    for model in &mut data {
        model.context_window = model.context_window.map(advertised_context_window);
    }

    // An image model on sd-server has no context window to report, running
    // or not.
    if let Some(target) = running
        && target.runtime == gglib_core::domain::RuntimeKind::Llama
        && let Some(model) = data
            .iter_mut()
            .find(|m| m.gglib_id == i64::from(target.model_id))
    {
        model.context_window = Some(advertised_context_window(target.effective_ctx));
    }

    // Append `{model}:{profile}` variants for profiles the user opted into
    // listing. Built from the base entries above, so they inherit the context
    // window each model would actually be served with.
    let variants = variant_entries(
        &data,
        state
            .settings
            .get()
            .await
            .inference_profiles
            .as_deref()
            .unwrap_or_default(),
    );
    data.extend(variants);
    Ok(data)
}

/// What this process says of itself: this build's version, and each of
/// gglib's own features whose routes answer here rather than refuse. Each
/// rests on the port the daemon hands its proxy, so a proxy without the
/// daemon (the phone's engine, a benchmark's, a test's) lists only `status`,
/// which every proxy answers.
fn served(state: &AppState) -> GglibInfo {
    let has = |feature| match feature {
        // A hub's chats are read, written and added to: the last is a turn.
        Feature::Chats => state.chats.is_some() && state.turns.is_some(),
        Feature::Runs => state.runs.is_some(),
        // Without a driver the drawing route answers `drawing_unavailable`.
        Feature::Drawing => state.images.is_some(),
        Feature::Status => true,
    };
    GglibInfo::this_build(Feature::ALL.into_iter().filter(|&f| has(f)))
}

#[cfg(test)]
#[path = "models_endpoint_tests.rs"]
mod tests;
