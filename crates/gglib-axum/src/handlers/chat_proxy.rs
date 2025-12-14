//! Chat completion proxy handler.
//!
//! Proxies chat completion requests to llama-server instances.
//! This enables the frontend to generate titles and chat through the API server.
//!
//! Supports both:
//! - Non-streaming: Returns JSON response
//! - Streaming: Returns SSE stream as pass-through

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::HttpError;
use crate::state::AppState;

/// Allowed port range for llama-server connections.
/// Prevents the endpoint from becoming a generic SSRF dialer.
const MIN_ALLOWED_PORT: u16 = 1024;
const MAX_ALLOWED_PORT: u16 = 65535;

/// Request body for chat completion proxy.
#[derive(Debug, Deserialize)]
pub struct ChatProxyRequest {
    /// The port of the llama-server to forward to.
    pub port: u16,
    /// The model identifier (not used for routing, just forwarded).
    #[serde(default)]
    pub model: String,
    /// The messages to send.
    pub messages: Vec<ChatMessage>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Optional max tokens.
    pub max_tokens: Option<u32>,
    /// Optional temperature.
    pub temperature: Option<f32>,
    /// Optional tools for function calling.
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Optional tool choice strategy.
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
}

/// A chat message in the request/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// Tool call ID (for tool role messages returning results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

/// Response from llama-server chat completion (non-streaming).
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Validate the requested port is within allowed range and corresponds
/// to a running server.
async fn validate_port(state: &AppState, port: u16) -> Result<(), HttpError> {
    // Basic range check
    if !(MIN_ALLOWED_PORT..=MAX_ALLOWED_PORT).contains(&port) {
        return Err(HttpError::BadRequest(format!(
            "Port {} is outside allowed range ({}-{})",
            port, MIN_ALLOWED_PORT, MAX_ALLOWED_PORT
        )));
    }

    // Check if the port matches a running server
    let servers = state.gui.list_servers().await;
    let port_in_use = servers.iter().any(|s| s.port == port);

    if !port_in_use {
        return Err(HttpError::BadRequest(format!(
            "No running server found on port {}. Start a server first.",
            port
        )));
    }

    Ok(())
}

/// Proxy chat completion requests to a running llama-server.
///
/// POST /api/chat
///
/// This handler forwards chat completion requests to the specified llama-server
/// instance and returns the response. Supports both streaming (SSE) and
/// non-streaming (JSON) modes.
///
/// # Security
///
/// - Port must be within allowed range (1024-65535)
/// - Port must correspond to a currently running server
pub async fn proxy_chat(
    State(state): State<AppState>,
    Json(request): Json<ChatProxyRequest>,
) -> Result<Response, HttpError> {
    // Validate the port
    validate_port(&state, request.port).await?;

    // Filter out messages with empty or whitespace-only content
    // EXCEPT: tool role messages (they return results) and assistant messages with tool_calls
    // This prevents Jinja template errors in llama-server
    let valid_messages: Vec<_> = request
        .messages
        .into_iter()
        .filter(|m| !m.content.trim().is_empty() || m.role == "tool" || m.tool_calls.is_some())
        .collect();

    if valid_messages.is_empty() {
        return Err(HttpError::BadRequest(
            "No valid messages to send. All messages have empty content.".into(),
        ));
    }

    // Build the llama-server URL
    let server_url = format!("http://127.0.0.1:{}/v1/chat/completions", request.port);

    // Build the forwarded request body
    let mut forward_body = serde_json::json!({
        "model": request.model,
        "messages": valid_messages,
        "stream": request.stream,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
    });

    // Add tools if provided
    if let Some(tools) = &request.tools
        && !tools.is_empty()
    {
        forward_body["tools"] = serde_json::json!(tools);
    }
    if let Some(tool_choice) = &request.tool_choice {
        forward_body["tool_choice"] = tool_choice.clone();
    }

    // DEBUG: Log the exact payload sent to llama-server
    let log_path = std::env::var("HOME")
        .map(|h| format!("{}/llama-request-debug.json", h))
        .unwrap_or_else(|_| "/tmp/llama-request-debug.json".to_string());

    if let Ok(json_str) = serde_json::to_string_pretty(&forward_body) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let log_entry = format!(
            "\n=== REQUEST {} ===\npath: /api/chat (proxy_chat)\n{}\n====================================\n",
            timestamp, json_str
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, log_entry.as_bytes()));
    }

    // Forward the request
    let client = Client::new();
    let response = client
        .post(&server_url)
        .header("Content-Type", "application/json")
        .json(&forward_body)
        .send()
        .await
        .map_err(|e| {
            HttpError::ServiceUnavailable(format!(
                "Failed to connect to llama-server on port {}: {}",
                request.port, e
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(HttpError::Internal(format!(
            "llama-server returned {}: {}",
            status, error_text
        )));
    }

    if request.stream {
        // Streaming mode: pass through SSE stream unchanged
        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(std::io::Error::other));

        let body = Body::from_stream(stream);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .unwrap()
            .into_response())
    } else {
        // Non-streaming mode: parse and return JSON
        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            HttpError::Internal(format!("Failed to parse llama-server response: {}", e))
        })?;

        Ok(Json(completion).into_response())
    }
}
