//! The paired machine's models, for this machine's surfaces: its list, one
//! model, and a load, each read from the far proxy through the tunnel with
//! the stored key.
//!
//! Unlike the chats, these are read rather than passed through: the list is
//! read into the far proxy's own `ModelInfo` and answered with the machine
//! it came from and what may be done to a model there, so the CLI and the
//! page parse far rows in one place. A far refusal keeps its status and its
//! code, as the chats' do, and a refused key is a `409`. A far machine on a
//! build too old to publish model ids is a `409` asking for an update.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use gglib_app_services::{FarError, FarProxy, PairedModels};
use serde::{Deserialize, Serialize};

use super::chats::refused;
use crate::error::HttpError;
use crate::state::AppState;

/// Body for `POST /api/remote/models/{model}/load`. Optional, as the far
/// route's is: an empty body loads the model at the context it would be
/// served with.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub(crate) struct LoadBody {
    /// A context size for the launch, in tokens.
    #[serde(default)]
    num_ctx: Option<u64>,
}

/// `GET /api/remote/models`.
pub(crate) async fn list_models(State(state): State<AppState>) -> Result<Response, HttpError> {
    list_models_via(&state.remote.far().await?).await
}

/// `GET /api/remote/models/{model}`.
pub(crate) async fn model_detail(
    State(state): State<AppState>,
    Path(model): Path<String>,
) -> Result<Response, HttpError> {
    model_detail_via(&state.remote.far().await?, &model).await
}

/// `POST /api/remote/models/{model}/load`.
pub(crate) async fn load_model(
    State(state): State<AppState>,
    Path(model): Path<String>,
    body: Option<Json<LoadBody>>,
) -> Result<Response, HttpError> {
    let LoadBody { num_ctx } = body.map(|Json(body)| body).unwrap_or_default();
    load_model_via(&state.remote.far().await?, &model, num_ctx).await
}

/// Every entry the far proxy lists, profile variants included, with the
/// machine they are on and that machine's row of the use-don't-change table.
pub(super) async fn list_models_via(far: &FarProxy) -> Result<Response, HttpError> {
    answer(far.models().await.map(|listed| {
        let machine = far.machine();
        PairedModels {
            actions: machine.actions(),
            machine,
            models: listed.data,
        }
    }))
}

pub(super) async fn model_detail_via(far: &FarProxy, model: &str) -> Result<Response, HttpError> {
    answer(far.lookup(model).await)
}

pub(super) async fn load_model_via(
    far: &FarProxy,
    model: &str,
    num_ctx: Option<u64>,
) -> Result<Response, HttpError> {
    answer(far.load(model, num_ctx).await)
}

/// A read, as this daemon answers it: the value as JSON, a far refusal in
/// this daemon's error shape, anything else as the error it is.
fn answer<T: Serialize>(read: Result<T, FarError>) -> Result<Response, HttpError> {
    match read {
        Ok(value) => Ok(Json(value).into_response()),
        Err(FarError::Refused {
            status,
            retry_after,
            body,
        }) => Ok(refused(status, retry_after, &body)),
        Err(FarError::Failed(error)) => Err(error.into()),
    }
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod models_tests;
