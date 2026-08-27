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

use axum::http::StatusCode;
use axum::{Json, extract::State, response::IntoResponse};
use tracing::{debug, error};

use crate::models::{ErrorResponse, ModelsResponse};
use crate::profiles::variant_entries;
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

/// List all models from the catalog in OpenAI format.
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
pub(crate) async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");

    match state.catalog_port.list_models().await {
        Ok(mut models) => {
            // Pinned mode refuses every other model, so advertising the rest
            // of the catalog would offer a BYOK client a choice that can only
            // come back as PinnedModelMismatch. Filtering the summaries here
            // rather than the finished response also keeps the variants below
            // correct for free — they are built from what survives.
            //
            // Profile variants of the pinned model stay: a profile changes
            // only the request body, never which model actually runs, so it
            // cannot trip the guard.
            if let Some(pinned) = state.runtime_port.pinned_model() {
                models.retain(|m| m.name == pinned);
            }

            let mut response = ModelsResponse::from_summaries(
                models,
                state.default_ctx,
                state.device_memory_readable,
            );

            // Apply safety margin to every model's context_window.
            for model in &mut response.data {
                model.context_window = model.context_window.map(advertised_context_window);
            }

            if let Some(target) = state.runtime_port.current_model().await
                && let Some(model) = response.data.iter_mut().find(|m| m.id == target.model_name)
            {
                model.context_window = Some(advertised_context_window(target.effective_ctx));
            }

            // Append `{model}:{profile}` variants for profiles the user opted
            // into listing. Built from the base entries above, so they inherit
            // the context window each model would actually be served with.
            let variants = variant_entries(
                &response.data,
                state
                    .settings
                    .get()
                    .await
                    .inference_profiles
                    .as_deref()
                    .unwrap_or_default(),
            );
            response.data.extend(variants);

            Json(response).into_response()
        }
        Err(e) => {
            error!("Failed to list models: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error(&format!(
                    "Failed to list models: {e}"
                ))),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
#[path = "models_endpoint_tests.rs"]
mod tests;
