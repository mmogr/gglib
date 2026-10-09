//! `GET /v1/models/{name}/detail`: one catalogued model, read in full.
//!
//! The `/v1/models` list carries what an `OpenAI` picker needs. A paired
//! machine showing this machine's models beside its own needs what the
//! inspector shows — architecture, quantization, provenance, tags, the raw
//! GGUF metadata — and the tunnel carries only this proxy, so the daemon's own
//! detail route is not somewhere it can go. Reading a model changes nothing
//! on this machine: the *use* side of ADR 0013's line.
//!
//! The identifier is resolved the way a chat request's is, so the answer is
//! the model a turn sent with the same string would reach. Three fields are
//! left out: the file path and the projector path, which describe this
//! machine's disk and nothing the reader can use, and the port, which is not
//! reachable from the other side of the tunnel.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::StatusCode};
use gglib_core::domain::{ModelDetailDto, ModelLookup};
use gglib_core::request_pipeline::{ModelRoute, resolve_route};
use tracing::{debug, error};

use crate::models::ErrorResponse;
use crate::profiles::configured_names;
use crate::server::AppState;

/// Answer what `name` resolves to, with the profile it named.
///
/// `name` is a catalogue id, an exact name, or either with a `:profile`
/// suffix. Unknown is 404 `model_not_found`, and so is any model but the pin
/// on a pinned endpoint, which `/v1/models` does not list either, whatever
/// suffix it carries. A suffix that names no configured profile is 404
/// `profile_not_found`, as on a chat request, once the model it is on is one
/// this endpoint answers for.
pub(crate) async fn model_detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    debug!(model = %name, "model detail requested");
    let settings = state.settings.get().await;
    let profiles = settings.inference_profiles.as_deref().unwrap_or_default();
    // An unknown suffix is answered only after the pin check, so a pinned
    // endpoint says no more about a model it hides than that it is not here.
    let (identifier, profile) =
        match resolve_route(&name, profiles, state.catalog_port.as_ref()).await {
            ModelRoute::Bare(model) => (model, Ok(None)),
            ModelRoute::Profiled { model, profile } => (model, Ok(Some(profile.name.clone()))),
            ModelRoute::ProfileNotFound { requested, suffix } => {
                let base = requested
                    .rsplit_once(':')
                    .map_or(requested, |(base, _)| base);
                let refusal = ErrorResponse::profile_not_found(
                    requested,
                    suffix,
                    configured_names(profiles).as_deref(),
                );
                (base, Err(refusal))
            }
        };

    let model = match state.catalog_port.model(identifier).await {
        Ok(Some(model)) => model,
        Ok(None) => return not_found(ErrorResponse::model_not_found(&name)),
        Err(e) => {
            error!("Failed to read model '{name}': {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error(&format!(
                    "Failed to read model '{name}': {e}"
                ))),
            )
                .into_response();
        }
    };
    if state
        .runtime_port
        .pinned()
        .is_some_and(|pin| pin.id != model.id)
    {
        return not_found(ErrorResponse::model_not_found(&name));
    }
    let profile = match profile {
        Ok(profile) => profile,
        Err(refusal) => return not_found(refusal),
    };

    // Serving means resident in either slot, matched by id: a name can belong
    // to more than one model.
    let is_serving = state
        .runtime_port
        .admission_snapshot()
        .slots
        .iter()
        .any(|slot| i64::from(slot.model_id) == model.id);
    let mut detail = ModelDetailDto {
        file_path: None,
        projector_path: None,
        ..ModelDetailDto::from_model(model, is_serving, None)
    };
    // A component's role and whether its file is there travel; where it sits
    // on this machine's disk does not, as for the projector.
    for component in &mut detail.components {
        component.path = None;
    }
    Json(ModelLookup { profile, detail }).into_response()
}

fn not_found(body: ErrorResponse) -> Response {
    (StatusCode::NOT_FOUND, Json(body)).into_response()
}
