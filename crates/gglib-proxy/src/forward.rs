//! Request forwarding to llama-server with proper streaming support.
//!
//! This module handles forwarding OpenAI API requests to the upstream
//! llama-server instance, preserving headers and streaming SSE responses.

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::TryStreamExt;
use reqwest::Client;
use tracing::{debug, error};

use crate::models::ErrorResponse;

/// Headers that should NOT be forwarded (hop-by-hop headers).
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    // Also strip these for security/correctness
    "host",
    "content-length",
    "authorization", // Don't forward auth to llama-server
];

/// Check if a header should be forwarded.
fn should_forward_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    !HOP_BY_HOP_HEADERS.contains(&lower.as_str())
}

/// Forward a chat completion request to the upstream llama-server.
///
/// # Arguments
///
/// * `client` - HTTP client to use for the request
/// * `upstream_url` - Full URL to the llama-server endpoint
/// * `headers` - Original request headers
/// * `body` - Request body bytes
/// * `is_streaming` - Whether this is a streaming request (affects response headers)
///
/// # Returns
///
/// The response from llama-server, with proper streaming if requested.
pub async fn forward_chat_completion(
    client: &Client,
    upstream_url: &str,
    headers: &HeaderMap,
    body: Bytes,
    is_streaming: bool,
) -> Response {
    debug!("Forwarding to {upstream_url}, streaming={is_streaming}");

    // Build the request to upstream
    let mut req_builder = client
        .post(upstream_url)
        .header("content-type", "application/json");

    // Forward allowed headers
    for (name, value) in headers.iter() {
        if should_forward_header(name.as_str())
            && let Ok(value_str) = value.to_str()
        {
            req_builder = req_builder.header(name.as_str(), value_str);
        }
    }

    // Send the request
    let response = match req_builder.body(body).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to connect to llama-server: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response();
        }
    };

    let status = response.status();

    // For errors, return the error body directly
    if !status.is_success() {
        let error_bytes = response.bytes().await.unwrap_or_default();
        return Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("content-type", "application/json")
            .body(Body::from(error_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    if is_streaming {
        // Stream SSE response
        forward_streaming_response(response).await
    } else {
        // Non-streaming: read full response
        forward_non_streaming_response(response).await
    }
}

/// Forward a streaming (SSE) response from llama-server.
async fn forward_streaming_response(response: reqwest::Response) -> Response {
    // Get the byte stream from the response
    let byte_stream = response.bytes_stream();

    // Map the stream to produce Result<Bytes, std::io::Error>
    // This is required for Body::from_stream
    let mapped_stream = byte_stream.map_err(std::io::Error::other);

    // Create the body from the stream
    let body = Body::from_stream(mapped_stream);

    // Build SSE response with proper headers
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no") // Disable nginx buffering
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Forward a non-streaming JSON response from llama-server.
async fn forward_non_streaming_response(response: reqwest::Response) -> Response {
    // Collect upstream headers we want to preserve
    let content_type = response
        .headers()
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));

    // Read the full body
    match response.bytes().await {
        Ok(body_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", content_type)
            .body(Body::from(body_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            error!("Failed to read upstream response: {e}");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_forward_header() {
        // Should forward
        assert!(should_forward_header("accept"));
        assert!(should_forward_header("content-type"));
        assert!(should_forward_header("x-custom-header"));

        // Should NOT forward
        assert!(!should_forward_header("connection"));
        assert!(!should_forward_header("host"));
        assert!(!should_forward_header("authorization"));
        assert!(!should_forward_header("transfer-encoding"));
    }
}
