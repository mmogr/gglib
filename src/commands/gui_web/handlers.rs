//! API route handlers for model management.
//!
//! This module contains the HTTP request handlers for the REST API,
//! implementing CRUD operations for models and server management.
//!
//! All business logic has been moved to the shared GuiBackend service
//! to eliminate duplication with the Tauri desktop app.

use crate::commands::download::{DownloadProgressEvent, ProgressThrottle};
use crate::commands::gui_web::state::AppState;
use crate::download::QueueSnapshot;
use crate::download::progress::build_queue_snapshot;
use crate::models::gui::{
    AddModelRequest, ApiResponse, AppSettings, CancelDownloadRequest, GuiModel,
    HfQuantizationsResponse, HfSearchRequest, HfSearchResponse, HfToolSupportResponse,
    ModelsDirectoryInfo, RemoveModelRequest, StartServerRequest, StartServerResponse,
    UpdateModelRequest, UpdateModelsDirectoryRequest, UpdateSettingsRequest,
};
use crate::utils::system::SystemMemoryInfo;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use serde::{Deserialize, Serialize};
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

/// Get system memory information for "Will it fit?" indicators
pub async fn get_system_memory_info() -> Result<Json<ApiResponse<SystemMemoryInfo>>, AppError> {
    let memory_info = crate::utils::system::get_system_memory_info();
    Ok(Json(ApiResponse::success(memory_info)))
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
    let throttle = ProgressThrottle::responsive_ui();

    let progress_model_id = model_id.clone();
    let completion_model_id = model_id.clone();
    let error_model_id = model_id.clone();
    let callback_model_id = model_id.clone();
    let callback_progress_tx = progress_tx.clone();
    let callback_throttle = throttle.clone();

    let callback: Box<dyn Fn(u64, u64) + Send + Sync> =
        Box::new(move |downloaded: u64, total: u64| {
            let Some(speed) = callback_throttle.should_emit_with_speed(downloaded, total) else {
                return;
            };
            let event =
                DownloadProgressEvent::progress(&callback_model_id, downloaded, total, speed);
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

/// Response for queue download containing position and shard count
#[derive(Debug, Serialize)]
pub struct QueueDownloadResponse {
    pub position: usize,
    pub shard_count: usize,
}

/// Add a download to the queue
pub async fn queue_download(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueueDownloadRequest>,
) -> Result<Json<ApiResponse<QueueDownloadResponse>>, AppError> {
    let (position, shard_count) = state
        .backend
        .queue_download(payload.model_id.clone(), payload.quantization.clone())
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    // Start the queue processor in a background task (if not already running)
    if state.backend.core().start_queue_if_idle() {
        let backend = state.backend.clone();
        tokio::spawn(async move {
            // process_queue runs until queue is empty, handles progress internally
            let _ = backend.core().downloads().process_queue().await;
            // Mark idle when done so future queues can start
            backend.core().mark_queue_idle();
        });
    }

    Ok(Json(ApiResponse::success(QueueDownloadResponse {
        position,
        shard_count,
    })))
}

/// Get the current download queue status
pub async fn get_download_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<QueueSnapshot>>, AppError> {
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

/// Reorder an item in the download queue
#[derive(Debug, Deserialize)]
pub struct ReorderQueueRequest {
    pub model_id: String,
    pub new_position: usize,
}

#[derive(Debug, Serialize)]
pub struct ReorderQueueResponse {
    pub actual_position: usize,
}

pub async fn reorder_download_queue(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ReorderQueueRequest>,
) -> Result<Json<ApiResponse<ReorderQueueResponse>>, AppError> {
    let actual_position = state
        .backend
        .reorder_download_queue(&payload.model_id, payload.new_position)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(ReorderQueueResponse {
        actual_position,
    })))
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

// HuggingFace Browser Handlers

/// Browse HuggingFace models with search and pagination
pub async fn browse_hf_models(
    State(state): State<Arc<AppState>>,
    Json(request): Json<HfSearchRequest>,
) -> Result<Json<ApiResponse<HfSearchResponse>>, AppError> {
    let response = state
        .backend
        .browse_hf_models(request)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(response)))
}

/// Get available quantizations for a HuggingFace model
pub async fn get_hf_quantizations(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Json<ApiResponse<HfQuantizationsResponse>>, AppError> {
    // URL decode the model_id (it will be URL encoded due to the slash)
    let decoded_model_id = urlencoding::decode(&model_id)
        .map_err(|e| AppError::BadRequest(format!("Invalid model ID encoding: {}", e)))?
        .into_owned();

    let response = state
        .backend
        .get_model_quantizations(&decoded_model_id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(response)))
}

/// Check if a HuggingFace model supports tool/function calling
pub async fn get_hf_tool_support(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Result<Json<ApiResponse<HfToolSupportResponse>>, AppError> {
    // URL decode the model_id (it will be URL encoded due to the slash)
    let decoded_model_id = urlencoding::decode(&model_id)
        .map_err(|e| AppError::BadRequest(format!("Invalid model ID encoding: {}", e)))?
        .into_owned();

    let response = state
        .backend
        .get_hf_tool_support(&decoded_model_id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(response)))
}

/// Stream download progress events via SSE
///
/// On connection, immediately sends the current queue snapshot so clients
/// know what's currently downloading (avoids race condition where download_started
/// event was sent before client subscribed).
pub async fn stream_progress(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, axum::Error>>> {
    // Get initial queue snapshot to send immediately
    let snapshot = state.backend.get_download_queue().await;
    let initial_event = build_queue_snapshot(&snapshot);
    let initial_json = serde_json::to_string(&initial_event).unwrap_or_default();

    // Create initial stream with the queue snapshot
    let initial = tokio_stream::once(Ok(Event::default().data(initial_json)));

    // Subscribe to broadcast channel for future events
    let rx = state.progress_tx.subscribe();
    let broadcast = BroadcastStream::new(rx).map(|msg| match msg {
        Ok(msg) => Ok(Event::default().data(msg)),
        Err(_) => Ok(Event::default().event("error").data("Stream error")),
    });

    // Chain initial event with broadcast stream
    let stream = initial.chain(broadcast);

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

/// Get filter options for model library (quantizations, param range, context range)
pub async fn get_model_filter_options(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<crate::services::database::ModelFilterOptions>>, AppError> {
    let options = state
        .backend
        .get_model_filter_options()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(options)))
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

/// Request payload for updating a message
#[derive(Debug, Deserialize)]
pub struct UpdateMessagePayload {
    pub content: String,
}

/// Response for delete message operation
#[derive(Debug, Serialize)]
pub struct DeleteMessageResponse {
    pub deleted_count: i64,
}

/// Update a message's content
pub async fn update_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<i64>,
    Json(payload): Json<UpdateMessagePayload>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    chat_history::update_message(state.backend.db_pool(), message_id, payload.content)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                AppError::NotFound(format!("Message {} not found", message_id))
            } else {
                AppError::DatabaseError(e.to_string())
            }
        })?;

    Ok(Json(ApiResponse::success("Message updated".to_string())))
}

/// Delete a message and all subsequent messages in the conversation
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<i64>,
) -> Result<Json<ApiResponse<DeleteMessageResponse>>, AppError> {
    let deleted_count =
        chat_history::delete_message_and_subsequent(state.backend.db_pool(), message_id)
            .await
            .map_err(|e| {
                if e.to_string().contains("not found") {
                    AppError::NotFound(format!("Message {} not found", message_id))
                } else {
                    AppError::DatabaseError(e.to_string())
                }
            })?;

    Ok(Json(ApiResponse::success(DeleteMessageResponse {
        deleted_count,
    })))
}

// ==================== Server Log Handlers ====================

use crate::utils::process::log_streamer::{ServerLogEntry, get_log_manager};

/// Response type for getting server logs
#[derive(Debug, Serialize)]
pub struct ServerLogsResponse {
    pub logs: Vec<ServerLogEntry>,
}

/// Get historical logs for a server (by port)
pub async fn get_server_logs(
    Path(port): Path<u16>,
) -> Result<Json<ApiResponse<ServerLogsResponse>>, AppError> {
    let log_manager = get_log_manager();
    let logs = log_manager.get_logs(port);
    Ok(Json(ApiResponse::success(ServerLogsResponse { logs })))
}

/// Stream server logs as Server-Sent Events (SSE)
pub async fn stream_server_logs(
    Path(port): Path<u16>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let log_manager = get_log_manager();
    let receiver = log_manager.subscribe();

    // Filter to only logs for the requested port
    let stream = BroadcastStream::new(receiver).filter_map(move |result| match result {
        Ok(entry) if entry.port == port => Some(Ok(
            Event::default().data(serde_json::to_string(&entry).unwrap_or_default())
        )),
        _ => None,
    });

    Sse::new(stream)
}

/// Clear logs for a specific server (by port)
pub async fn clear_server_logs(
    Path(port): Path<u16>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    let log_manager = get_log_manager();
    log_manager.clear_logs(port);
    Ok(Json(ApiResponse::success("Logs cleared".to_string())))
}

// ==================== MCP Server Handlers ====================

use crate::services::mcp::{McpServerConfig, McpServerInfo, McpTool, McpToolResult};
use std::collections::HashMap;

/// List all MCP servers with their current status
pub async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<McpServerInfo>>>, AppError> {
    let servers = state
        .backend
        .core()
        .mcp()
        .list_servers_with_status()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(servers)))
}

/// Add a new MCP server configuration
pub async fn add_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<ApiResponse<McpServerConfig>>, AppError> {
    let server = state
        .backend
        .core()
        .mcp()
        .add_server(config)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(server)))
}

/// Get a specific MCP server by ID
pub async fn get_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<McpServerInfo>>, AppError> {
    let info = state
        .backend
        .core()
        .mcp()
        .get_server_info(&id)
        .await
        .map_err(|e| AppError::NotFound(e.to_string()))?;

    Ok(Json(ApiResponse::success(info)))
}

/// Update an MCP server configuration
pub async fn update_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(config): Json<McpServerConfig>,
) -> Result<Json<ApiResponse<McpServerConfig>>, AppError> {
    let updated = state
        .backend
        .core()
        .mcp()
        .update_server(&id, config)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success(updated)))
}

/// Remove an MCP server configuration
pub async fn remove_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state
        .backend
        .core()
        .mcp()
        .remove_server(&id)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(Json(ApiResponse::success("Server removed".to_string())))
}

/// Start an MCP server
pub async fn start_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<McpTool>>>, AppError> {
    let tools = state
        .backend
        .core()
        .mcp()
        .start_server(&id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(tools)))
}

/// Stop an MCP server
pub async fn stop_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, AppError> {
    state
        .backend
        .core()
        .mcp()
        .stop_server(&id)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success("Server stopped".to_string())))
}

/// Get all tools from all running MCP servers
pub async fn list_mcp_tools(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<(String, Vec<McpTool>)>>>, AppError> {
    let tools = state.backend.core().mcp().list_all_tools().await;

    Ok(Json(ApiResponse::success(tools)))
}

/// Request body for calling an MCP tool
#[derive(Debug, Deserialize)]
pub struct CallMcpToolRequest {
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Call a tool on an MCP server
pub async fn call_mcp_tool(
    State(state): State<Arc<AppState>>,
    Path((server_id, tool_name)): Path<(String, String)>,
    Json(request): Json<CallMcpToolRequest>,
) -> Result<Json<ApiResponse<McpToolResult>>, AppError> {
    let result = state
        .backend
        .core()
        .mcp()
        .call_tool(&server_id, &tool_name, request.arguments)
        .await
        .map_err(|e| AppError::ServerError(e.to_string()))?;

    Ok(Json(ApiResponse::success(result)))
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
