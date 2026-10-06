//! `POST /api/chat`: a chat's title, asked of the llama-server the chat runs
//! on.
//!
//! The page sends the conversation with its instruction as the last message,
//! the temperature, and the most tokens the title may take. The answer is the
//! model's text as it gave it, which the page cleans into the title.
//!
//! The request is shaped by [`request_pipeline::apply()`], as a turn of the
//! chat is, and posted to the server on the port, which is where the chat's
//! own turns go: the proxy may be stopped while a chat is open. It carries no
//! tools and is never streamed, and a body naming any key but the four below
//! is refused.

use axum::Json;
use axum::extract::State;
use gglib_app_services::types::ServerInfo;
use gglib_core::domain::InferenceConfig;
use gglib_core::request_pipeline::{self, ModelContext, SamplingLayers};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

/// The thinking budget a title is asked for with: none. `max_tokens` counts
/// thinking too, so a model that thinks first would spend the title's cap
/// before the title's first word.
const NO_THINKING: i32 = 0;

/// Request body for a chat's title.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatTitleRequest {
    /// The port of the llama-server the chat runs on.
    pub port: u16,
    /// The conversation, then the instruction to title it.
    pub messages: Vec<ChatTitleMessage>,
    /// The temperature the title is sampled at.
    pub temperature: f32,
    /// The most tokens the title may take.
    pub max_tokens: u32,
}

/// One message of a [`ChatTitleRequest`]: who said it, and its text.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChatTitleMessage {
    pub role: String,
    pub content: String,
}

/// Ask the model on a port for a chat's title, and answer with its text:
/// empty when it gave none.
/// POST /api/chat
///
/// # Errors
///
/// `422` when the body is not a title request. `400` when no server this
/// daemon started is on the port, when no message has text, or when the
/// conversation cannot be made to fit the model's context; `503` when
/// llama-server cannot be reached; `500` when it refuses or its reply is not
/// JSON.
pub(crate) async fn generate(
    State(state): State<AppState>,
    Json(request): Json<ChatTitleRequest>,
) -> Result<Json<String>, HttpError> {
    let server = validate_port(&state, request.port).await?;
    Ok(Json(title_from(&state, &server, request).await?))
}

/// `server`'s answer to a title request, shaped for the model it serves and
/// by the settings' own sampling defaults.
///
/// Apart from [`generate`] because no test has a running server for
/// `validate_port` to find.
///
/// # Errors
///
/// As [`generate`], the port check apart.
async fn title_from(
    state: &AppState,
    server: &ServerInfo,
    request: ChatTitleRequest,
) -> Result<String, HttpError> {
    // By its id: the catalog row the server was started from.
    let model = server.model_id.to_string();
    let ctx = request_pipeline::resolve(state.catalog.as_ref(), Some(&model)).await;
    let global = state
        .core
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.inference_defaults);
    let body = model_request(request, &ctx, global)?;
    ask(server.port, &body).await
}

/// The request llama-server is sent for a title.
///
/// The messages that have text, and a tool's result whether it has or not;
/// the caller's temperature and cap; [`NO_THINKING`]. The pipeline then
/// shapes it as it shapes a turn of the chat, with those three as the top of
/// its sampling ladder whatever `trust_client_sampling` says of an outside
/// client: the body is this daemon's own page's, and typed.
///
/// # Errors
///
/// `400` when no message is left to send, or the conversation cannot be made
/// to fit the model's context.
fn model_request(
    request: ChatTitleRequest,
    ctx: &ModelContext,
    global: Option<InferenceConfig>,
) -> Result<Value, HttpError> {
    // A message with no text fails some chat templates outright.
    let messages: Vec<ChatTitleMessage> = request
        .messages
        .into_iter()
        .filter(|m| m.role == "tool" || !m.content.trim().is_empty())
        .collect();
    if messages.is_empty() {
        return Err(HttpError::BadRequest(
            "No valid messages to send. All messages have empty content.".into(),
        ));
    }

    let mut fields = InferenceConfig {
        temperature: Some(request.temperature),
        max_tokens: Some(request.max_tokens),
        reasoning_budget_tokens: Some(NO_THINKING),
        ..InferenceConfig::default()
    }
    .to_openai_json_patch();
    fields.insert("messages".to_owned(), serde_json::json!(messages));
    fields.insert("stream".to_owned(), Value::Bool(false));
    let mut body = Value::Object(fields);

    request_pipeline::apply(
        &mut body,
        ctx,
        &SamplingLayers {
            global,
            trust_client_sampling: true,
            ..SamplingLayers::default()
        },
        ctx.context_budget(),
    )
    .map_err(|e| HttpError::BadRequest(e.to_string()))?;
    Ok(body)
}

/// Post `body` to the llama-server on `port`, and return the text of its
/// reply: empty when the reply has none.
///
/// # Errors
///
/// `503` when the server cannot be reached; `500` when it answers anything
/// but success, or its reply is not JSON.
async fn ask(port: u16, body: &Value) -> Result<String, HttpError> {
    // Built by `loopback`: llama-server is on this machine, never behind a proxy.
    let response = gglib_proxy::loopback::client()
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .json(body)
        .send()
        .await
        .map_err(|e| {
            HttpError::ServiceUnavailable(format!(
                "Failed to connect to llama-server on port {port}: {e}"
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(HttpError::Internal(format!(
            "llama-server returned {status}: {error_text}"
        )));
    }

    let reply: Value = response
        .json()
        .await
        .map_err(|e| HttpError::Internal(format!("Failed to parse llama-server response: {e}")))?;
    Ok(reply
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

#[cfg(test)]
#[path = "chat_title_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "chat_title_reply_tests.rs"]
mod reply_tests;
