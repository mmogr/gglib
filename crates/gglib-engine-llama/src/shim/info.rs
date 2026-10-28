//! What the shim says of itself and its model: `GET /props`, `GET /health`,
//! `GET /slots`, `POST /slots/{id}`, `POST /apply-template`, and the 404 of
//! everything else.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use gglib_core::domain::engine_pins::PINNED_LLAMA_RELEASE;
use serde_json::{Value, json};

use super::frames::{self, ErrorKind};
use crate::decoder::Sampling;
use crate::engine::Engine;
use crate::render::Chat;

/// What llama-server answers while it has no model to serve.
const LOADING: &str = "Loading model";

/// `GET /props`: the model served, its context and template, and the
/// sampling a request that names nothing gets, under llama-server's keys.
///
/// `build_info` is the pin. `chat_template_caps` says only what holds by
/// construction, whatever the template: it is given string content and
/// never tools, tool calls or typed content parts. The caps llama-server
/// finds by executing the template are left out, so a reader holds them as
/// not reported rather than as false (ADR 0007).
pub(crate) async fn props(State(engine): State<Arc<Engine>>) -> Response {
    let Some(resident) = engine.resident() else {
        return frames::error(ErrorKind::Unavailable, LOADING);
    };
    let loaded = &resident.loaded;
    Json(json!({
        "default_generation_settings": {
            "n_ctx": loaded.n_ctx,
            "params": Sampling::default().to_json(),
        },
        "total_slots": 1,
        "model_alias": resident.name,
        "model_path": resident.path.display().to_string(),
        "modalities": {"vision": false, "video": false, "audio": false},
        "endpoint_slots": false,
        "endpoint_props": false,
        "endpoint_metrics": false,
        "chat_template": resident.template.source(),
        "chat_template_caps": {
            "supports_string_content": true,
            "supports_typed_content": false,
            "supports_tools": false,
            "supports_tool_calls": false,
        },
        "bos_token": loaded.vocab.bos,
        "eos_token": loaded.vocab.eos,
        "build_info": PINNED_LLAMA_RELEASE,
        "is_sleeping": false,
    }))
    .into_response()
}

/// `GET /health`: ok while a model is held, 503 otherwise, as llama-server
/// answers while its model loads.
pub(crate) async fn health(State(engine): State<Arc<Engine>>) -> Response {
    if engine.resident().is_some() {
        Json(json!({"status": "ok"})).into_response()
    } else {
        frames::error(ErrorKind::Unavailable, LOADING)
    }
}

/// `GET /slots`: 501, as llama-server started without its slots endpoint
/// answers, which stops gglib-proxy's slots poller for good.
pub(crate) async fn slots() -> Response {
    frames::error(
        ErrorKind::NotSupported,
        "This server does not support slots endpoint.",
    )
}

/// `POST /slots/{id}`: 501, as llama-server started without
/// `--slot-save-path` answers a slot's save, restore or erase.
pub(crate) async fn slot_action() -> Response {
    frames::error(
        ErrorKind::NotSupported,
        "This server does not support slots action. Start it with `--slot-save-path`",
    )
}

/// A path or method the shim does not serve: llama-server's 404.
pub(crate) async fn not_found() -> Response {
    frames::error(ErrorKind::NotFound, "File Not Found")
}

/// `POST /apply-template`: the prompt a chat renders to, unsent.
pub(crate) async fn apply_template(State(engine): State<Arc<Engine>>, body: Bytes) -> Response {
    let Ok(body) = serde_json::from_slice::<Value>(&body) else {
        return frames::error(ErrorKind::InvalidRequest, "the request body is not JSON");
    };
    let Some(resident) = engine.resident() else {
        return frames::error(ErrorKind::Unavailable, LOADING);
    };
    match Chat::read(&body).and_then(|chat| chat.render(&resident.template)) {
        Ok(prompt) => Json(json!({"prompt": prompt})).into_response(),
        Err(why) => frames::error(ErrorKind::InvalidRequest, &why),
    }
}
