//! `POST /v1/models/{name}/load`: have a model resident before a turn needs it.
//!
//! The one route the *use* side of ADR 0013 needed that the proxy did not
//! already have. A turn loads its model on the way in, so a client that only
//! ever asks questions never wanted this; `gglib serve --remote` does — the
//! reason to run it is that the first turn should not wait — and the tunnel
//! carries only this proxy, so the daemon's own start endpoint is not
//! somewhere a paired machine can go.
//!
//! It admits exactly the way a chat completion does — the same queue, the
//! same batching, the same context rules — and then drops the lease at once,
//! the posture [`Admission::into_target`] exists for: the model is resident,
//! and evictable from this moment on. Nothing about what is *on* the machine
//! changes, which is the line the use side does not cross.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{Json, http::StatusCode};
use gglib_core::ports::{Admission, LaunchOverrides};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::server::{AppState, handle_runtime_error};

/// What `POST /v1/models/{name}/load` accepts. Every field optional; an
/// empty body loads the model at the context it would be served with.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct LoadRequest {
    /// A context size for the launch, as a chat request's `num_ctx` is.
    #[serde(default)]
    pub num_ctx: Option<u64>,
}

/// What it answers once the model is resident.
#[derive(Debug, Serialize)]
pub(crate) struct LoadResponse {
    /// The model, by the name it was asked for.
    pub model: String,
    /// Whether this call started it, as opposed to finding it running.
    pub started: bool,
    /// The context it is serving with.
    pub context: u64,
}

/// Load `name`, or find it loaded, and say which.
///
/// Absence, an embedding-only model, a queue that never reached the front:
/// every refusal is [`handle_runtime_error`]'s, worded exactly as the same
/// condition is worded on a chat request, so a person who has seen one has
/// seen both.
pub(crate) async fn load_model(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Option<Json<LoadRequest>>,
) -> Response {
    let request = body.map(|Json(req)| req).unwrap_or_default();
    info!(model = %name, num_ctx = ?request.num_ctx, "POST /v1/models/{{name}}/load");
    match state
        .runtime_port
        .admit(
            &name,
            request.num_ctx,
            state.default_ctx,
            LaunchOverrides::default(),
        )
        .await
    {
        Ok(admission) => {
            let target = Admission::into_target(admission);
            (
                StatusCode::OK,
                Json(LoadResponse {
                    model: target.model_name,
                    started: target.just_started,
                    context: target.effective_ctx,
                }),
            )
                .into_response()
        }
        Err(e) => handle_runtime_error(e),
    }
}
