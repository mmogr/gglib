//! API route handlers for model management.
//!
//! This module contains the HTTP request handlers for the REST API,
//! implementing CRUD operations for models and server management.
//!
//! All business logic has been moved to the shared GuiBackend service
//! to eliminate duplication with the Tauri desktop app.

use crate::commands::download::{DownloadProgressEvent, ProgressThrottle};
use crate::commands::gui_web::state::AppState;
use crate::models::gui::{
    AddModelRequest, ApiResponse, AppSettings, CancelDownloadRequest, GuiModel,
    ModelsDirectoryInfo, RemoveModelRequest, StartServerRequest, StartServerResponse,
    UpdateModelRequest, UpdateModelsDirectoryRequest, UpdateSettingsRequest,
};
use crate::services::core::DownloadQueueStatus;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use serde::Deserialize;
use sqlx::Error as SqlxError;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, error};

/// List all models with their serving status
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<GuiModel>>>, AppError> {
    let models = state
        .backend
        .list_models()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(models)))
}

/// Get a specific model by ID
pub async fn get_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
) -> Result<Json<ApiResponse<GuiModel>>, AppError> {
    let model = state
        .backend
        .get_model(id)
        .await
        .map_err(|e| AppError::NotFound(e.to_string()))?;

    Ok(Json(ApiResponse::success(model)))
}

/// Start serving a model
pub async fn start_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
    Json(payload): Json<StartServerRequest>,
) -> Result<Json<ApiResponse<StartServerResponse>>, AppError> {
    let response = state
        .backend
        .start_server(id, payload)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(response)))
}

/// Stop serving a model
pub async fn stop_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let message = state
        .backend
        .stop_model(id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(message)))
}

/// List all running servers
pub async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::utils::ServerInfo>>>, AppError> {
    let servers = state
        .backend
        .list_servers()
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(servers)))
}

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "gglib-web",
    }))
}

/// Return current models directory configuration for the settings dialog
pub async fn get_models_directory(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<ModelsDirectoryInfo>>, AppError> {
    let info = state
        .backend
        .get_models_directory_info()
        .map_err(|e| AppError::ServerError(e.to_string()))?;
    Ok(Json(ApiResponse::success(info)))
}

/// Update and persist the models directory selection from the settings dialog
pub async fn update_models_directory(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateModelsDirectoryRequest>,
) -> Result<Json<ApiResponse<ModelsDirectoryInfo>>, AppError> {
    let info = state
        .backend
        .update_models_directory(payload.path)
        .map_err(|e| AppError::ServerError(e.to_string()))?;
    Ok(Json(ApiResponse::success(info)))
}

/// Get application settings
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AppSettings>>, AppError> {
    let settings = state
        .backend
        .get_settings()
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;
    Ok(Json(ApiResponse::success(settings)))
}

/// Update application settings
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateSettingsRequest>,
) -> Result<Json<ApiResponse<AppSettings>>, AppError> {
    let settings = state
        .backend
        .update_settings(payload)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;
    Ok(Json(ApiResponse::success(settings)))
}

/// Add a model to the database
pub async fn add_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddModelRequest>,
) -> Result<Json<ApiResponse<GuiModel>>, AppError> {
    let gui_model = state
        .backend
        .add_model(payload)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(gui_model)))
}

/// Update a model in the database
pub async fn update_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
    Json(payload): Json<UpdateModelRequest>,
) -> Result<Json<ApiResponse<GuiModel>>, AppError> {
    let updated_model = state
        .backend
        .update_model(id, payload)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(updated_model)))
}

/// Remove a model from the database
pub async fn remove_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u32>,
    body: Option<Json<RemoveModelRequest>>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let request = body
        .map(|b| b.0)
        .unwrap_or_else(|| RemoveModelRequest { force: false });

    let message = state
        .backend
        .remove_model(id, request)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(message)))
}

/// Download a model from HuggingFace Hub
pub async fn download_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<crate::models::gui::DownloadModelRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let progress_tx = state.progress_tx.clone();
    let model_id = payload.model_id.clone();

    // Create a progress callback that sends updates to the broadcast channel
    let start_time = std::time::Instant::now();
    let throttle = ProgressThrottle::responsive_ui();

    let progress_model_id = model_id.clone();
    let completion_model_id = model_id.clone();
    let error_model_id = model_id.clone();
    let callback_model_id = model_id.clone();
    let callback_progress_tx = progress_tx.clone();
    let callback_throttle = throttle.clone();

    let callback: Box<dyn Fn(u64, u64) + Send + Sync> =
        Box::new(move |downloaded: u64, total: u64| {
            if !callback_throttle.should_emit(downloaded, total) {
                return;
            }
            let event =
                DownloadProgressEvent::progress(&callback_model_id, downloaded, total, start_time);
            let _ = callback_progress_tx.send(event.to_json_string());
        });

    let _ = progress_tx.send(DownloadProgressEvent::starting(&progress_model_id).to_json_string());

    match state
        .backend
        .download_model(payload.model_id, payload.quantization, Some(&callback))
        .await
    {
        Ok(message) => {
            let _ = progress_tx.send(
                DownloadProgressEvent::completed(&completion_model_id, Some(&message))
                    .to_json_string(),
            );
            Ok(Json(ApiResponse::success(message)))
        }
        Err(e) => {
            let error_msg = e.to_string();
            let _ = progress_tx
                .send(DownloadProgressEvent::errored(&error_model_id, &error_msg).to_json_string());
            Err(AppError::ServerError(error_msg))
        }
    }
}

/// Cancel an in-progress download
pub async fn cancel_download(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CancelDownloadRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let model_id = payload.model_id.clone();

    state
        .backend
        .cancel_download(&model_id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    let _ = state.progress_tx.send(
        DownloadProgressEvent::errored(&model_id, "Download cancelled by user").to_json_string(),
    );

    Ok(Json(ApiResponse::success(format!(
        "Download '{}' cancelled",
        model_id
    ))))
}

// Download Queue Handlers

/// Queue a download to be processed
#[derive(Debug, Deserialize)]
pub struct QueueDownloadRequest {
    pub model_id: String,
    pub quantization: Option<String>,
}

/// Add a download to the queue
pub async fn queue_download(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueueDownloadRequest>,
) -> Result<Json<ApiResponse<usize>>, AppError> {
    let position = state
        .backend
        .queue_download(payload.model_id, payload.quantization)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(position)))
}

/// Get the current download queue status
pub async fn get_download_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<DownloadQueueStatus>>, AppError> {
    let status = state.backend.get_download_queue().await;
    Ok(Json(ApiResponse::success(status)))
}

/// Remove an item from the download queue
#[derive(Debug, Deserialize)]
pub struct RemoveFromQueueRequest {
    pub model_id: String,
}

pub async fn remove_from_download_queue(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RemoveFromQueueRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state
        .backend
        .remove_from_download_queue(&payload.model_id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(format!(
        "Removed '{}' from download queue",
        payload.model_id
    ))))
}

/// Clear all failed downloads
pub async fn clear_failed_downloads(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state.backend.clear_failed_downloads().await;
    Ok(Json(ApiResponse::success(
        "Cleared failed downloads".to_string(),
    )))
}

/// Stream download progress events via SSE
pub async fn stream_progress(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.progress_tx.subscribe();
    let stream = BroadcastStream::new(rx).map(|msg| match msg {
        Ok(msg) => Ok(Event::default().data(msg)),
        Err(_) => Ok(Event::default().event("error").data("Stream error")),
    });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Get proxy status
pub async fn get_proxy_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let status = state
        .backend
        .get_proxy_status()
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(status)))
}

/// Start the OpenAI-compatible proxy
pub async fn start_proxy(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let host = payload["host"].as_str().unwrap_or("127.0.0.1").to_string();
    let port = payload["port"].as_u64().unwrap_or(8080) as u16;
    let start_port = payload["start_port"].as_u64().unwrap_or(9000) as u16;
    let default_context = payload["default_context"].as_u64().unwrap_or(8192);

    let message = state
        .backend
        .start_proxy(host, port, start_port, default_context)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(message)))
}

/// Stop the OpenAI-compatible proxy
pub async fn stop_proxy(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let message = state
        .backend
        .stop_proxy()
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(message)))
}

/// Proxy chat completions to a running llama-server
///
/// Forwards POST requests from the frontend to the llama-server's OpenAI-compatible
/// /v1/chat/completions endpoint. Supports both streaming and non-streaming responses.
pub async fn chat_proxy(
    State(_state): State<Arc<AppState>>,
    Json(mut payload): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    debug!("Chat proxy request received");

    // Extract the port from the request body
    let port = payload
        .get("port")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| AppError::ServerError("Missing 'port' field in request".to_string()))?
        as u16;

    // Check if streaming is requested
    let is_streaming = payload
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    debug!(port = %port, streaming = %is_streaming, "Forwarding to llama-server");

    // Remove the port field before forwarding
    if let Some(obj) = payload.as_object_mut() {
        obj.remove("port");
    }

    // Build target URL
    let target_url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
    debug!(target_url = %target_url, "Proxying request");

    // Forward the request
    let client = reqwest::Client::new();
    let response = client
        .post(&target_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "Proxy request failed");
            AppError::ServerError(format!("Failed to proxy request: {}", e))
        })?;

    debug!(status = %response.status(), "Got response from llama-server");

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        error!(status = %status, error = %error_text, "llama-server error");
        return Err(AppError::ServerError(format!(
            "llama-server error ({}): {}",
            status, error_text
        )));
    }

    if is_streaming {
        // Stream the response back to the client using SSE
        debug!("Streaming chat response");

        let byte_stream = response.bytes_stream();

        // Convert the byte stream to a stream of strings for SSE forwarding
        let sse_stream = byte_stream.map(|result| {
            result
                .map(|bytes| {
                    // Forward the raw SSE data as-is
                    let data = String::from_utf8_lossy(&bytes);
                    Event::default().data(data.trim())
                })
                .map_err(|e| {
                    error!(error = %e, "Stream error");
                    std::io::Error::other(e)
                })
        });

        Ok(Sse::new(sse_stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response())
    } else {
        // Non-streaming: forward the complete JSON response
        let response_json: serde_json::Value = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse response");
            AppError::ServerError(format!("Failed to parse response: {}", e))
        })?;

        debug!("Chat proxy successful");
        Ok(Json(response_json).into_response())
    }
}

// Tag Management Handlers

/// List all unique tags used across all models
pub async fn list_tags(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let tags = state
        .backend
        .list_tags()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tags)))
}

#[derive(serde::Deserialize)]
pub struct AddTagRequest {
    pub tag: String,
}

/// Add a tag to a model
pub async fn add_model_tag(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<u32>,
    Json(payload): Json<AddTagRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state
        .backend
        .add_model_tag(model_id, payload.tag)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(
        "Tag added to model successfully".to_string(),
    )))
}

#[derive(serde::Deserialize)]
pub struct RemoveTagRequest {
    pub tag: String,
}

/// Remove a tag from a model
pub async fn remove_model_tag(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<u32>,
    Json(payload): Json<RemoveTagRequest>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state
        .backend
        .remove_model_tag(model_id, payload.tag)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(
        "Tag removed from model successfully".to_string(),
    )))
}

/// Get all tags for a specific model
pub async fn get_model_tags(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<u32>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let tags = state
        .backend
        .get_model_tags(model_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tags)))
}

// ===== Chat History Handlers =====

use crate::services::chat_history::{
    self, Conversation, CreateConversationRequest, CreateMessageRequest, Message,
};

#[derive(Debug, Deserialize)]
pub struct UpdateConversationPayload {
    pub title: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<Option<String>>,
}

/// Get all conversations
pub async fn list_conversations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<Conversation>>>, AppError> {
    let conversations = chat_history::get_conversations(state.backend.db_pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(conversations)))
}

/// Get a specific conversation
pub async fn get_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<i64>,
) -> Result<Json<ApiResponse<Conversation>>, AppError> {
    let conversation = chat_history::get_conversation(state.backend.db_pool(), conversation_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound("Conversation not found".to_string()))?;

    Ok(Json(ApiResponse::success(conversation)))
}

/// Create a new conversation
pub async fn create_conversation(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateConversationRequest>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    let conversation_id = chat_history::create_conversation(
        state.backend.db_pool(),
        payload.title,
        payload.model_id,
        payload.system_prompt,
    )
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(conversation_id)))
}

/// Update a conversation's metadata
pub async fn update_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<i64>,
    Json(payload): Json<UpdateConversationPayload>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let UpdateConversationPayload {
        title,
        system_prompt,
    } = payload;

    match chat_history::update_conversation(
        state.backend.db_pool(),
        conversation_id,
        title,
        system_prompt,
    )
    .await
    {
        Ok(_) => {}
        Err(error) => {
            let is_missing_conversation = error
                .downcast_ref::<SqlxError>()
                .map(|sqlx_error| matches!(sqlx_error, SqlxError::RowNotFound))
                .unwrap_or(false);

            if is_missing_conversation {
                return Err(AppError::NotFound("Conversation not found".to_string()));
            }

            return Err(AppError::DatabaseError(error.to_string()));
        }
    }

    Ok(Json(ApiResponse::success(
        "Conversation updated".to_string(),
    )))
}

/// Delete a conversation
pub async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<i64>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    chat_history::delete_conversation(state.backend.db_pool(), conversation_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(
        "Conversation deleted".to_string(),
    )))
}

/// Get messages for a conversation
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<i64>,
) -> Result<Json<ApiResponse<Vec<Message>>>, AppError> {
    let messages = chat_history::get_messages(state.backend.db_pool(), conversation_id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(messages)))
}

/// Save a message to a conversation
pub async fn save_message(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateMessageRequest>,
) -> Result<Json<ApiResponse<i64>>, AppError> {
    let message_id = chat_history::save_message(
        state.backend.db_pool(),
        payload.conversation_id,
        payload.role,
        payload.content,
    )
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(message_id)))
}

/// Custom error type for API handlers
#[derive(Debug)]
pub enum AppError {
    DatabaseError(String),
    NotFound(String),
    ServerError(String),
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DatabaseError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::ServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(ApiResponse::<()>::error(error_message));
        (status, body).into_response()
    }
}
