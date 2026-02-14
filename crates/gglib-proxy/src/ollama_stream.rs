//! SSE ↔ NDJSON streaming adapter for Ollama compatibility.
//!
//! Ollama streams newline-delimited JSON (NDJSON), while llama-server
//! streams Server-Sent Events (SSE). This module translates between the
//! two on the fly.

use std::time::Instant;

use axum::{body::Body, http::StatusCode, response::{IntoResponse, Response}};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use tracing::warn;

use crate::ollama_models::*;

/// Translate an upstream SSE streaming response into Ollama NDJSON for `/api/chat`.
pub(crate) async fn stream_chat_response(
    upstream: reqwest::Response,
    model: String,
    start: Instant,
) -> Response {
    let byte_stream = upstream.bytes_stream();
    let ndjson_stream = sse_to_ndjson(byte_stream, model, start, StreamKind::Chat);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/x-ndjson")
        .header("transfer-encoding", "chunked")
        .body(Body::from_stream(ndjson_stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Translate an upstream SSE streaming response into Ollama NDJSON for `/api/generate`.
pub(crate) async fn stream_generate_response(
    upstream: reqwest::Response,
    model: String,
    start: Instant,
) -> Response {
    let byte_stream = upstream.bytes_stream();
    let ndjson_stream = sse_to_ndjson(byte_stream, model, start, StreamKind::Generate);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/x-ndjson")
        .header("transfer-encoding", "chunked")
        .body(Body::from_stream(ndjson_stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Clone, Copy)]
enum StreamKind {
    Chat,
    Generate,
}

/// State threaded through the `unfold` stream.
struct NdjsonState<S> {
    stream: S,
    buf: BytesMut,
    model: String,
    start: Instant,
    eval_count: u32,
    kind: StreamKind,
    done: bool,
    /// Actual token counts captured from the upstream `usage` SSE chunk
    /// (available when `stream_options: {include_usage: true}` was sent).
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

/// Convert an SSE byte stream from llama-server into an NDJSON byte stream.
///
/// SSE format:  `data: {"choices":[{"delta":{"content":"hi"}}]}\n\n`
/// NDJSON chat: `{"model":"...","message":{"role":"assistant","content":"hi"},"done":false}\n`
fn sse_to_ndjson<S>(
    byte_stream: S,
    model: String,
    start: Instant,
    kind: StreamKind,
) -> impl Stream<Item = Result<Bytes, std::io::Error>>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let state = NdjsonState {
        stream: byte_stream.boxed(),
        buf: BytesMut::new(),
        model,
        start,
        eval_count: 0,
        kind,
        done: false,
        prompt_tokens: None,
        completion_tokens: None,
    };

    futures_util::stream::unfold(state, |mut st| async move {
        if st.done {
            return None;
        }

        loop {
            // Try to extract a complete SSE line from the buffer.
            if let Some(line_end) = find_newline(&st.buf) {
                let line = st.buf.split_to(line_end);
                let line_str = String::from_utf8_lossy(&line);
                let trimmed = line_str.trim();

                // Skip empty lines and SSE comments.
                if trimmed.is_empty() || trimmed.starts_with(':') {
                    continue;
                }

                if let Some(data) = trimmed.strip_prefix("data: ") {
                    let data = data.trim();

                    // SSE termination signal.
                    if data == "[DONE]" {
                        st.done = true;
                        let out = emit_final_chunk(
                            &st.model,
                            st.start,
                            st.completion_tokens.unwrap_or(st.eval_count),
                            st.prompt_tokens.unwrap_or(0),
                            st.kind,
                        );
                        return Some((Ok(Bytes::from(out)), st));
                    }

                    // Parse the OpenAI chunk and translate.
                    if let Ok(openai) = serde_json::from_str::<serde_json::Value>(data) {
                        // Capture usage data if present (from stream_options: {include_usage: true}).
                        if let Some(usage) = openai.get("usage") {
                            if let Some(pt) = usage["prompt_tokens"].as_u64() {
                                st.prompt_tokens = Some(pt as u32);
                            }
                            if let Some(ct) = usage["completion_tokens"].as_u64() {
                                st.completion_tokens = Some(ct as u32);
                            }
                        }

                        let content = openai["choices"][0]["delta"]["content"]
                            .as_str()
                            .unwrap_or("");

                        if !content.is_empty() {
                            st.eval_count += 1;
                        }

                        let out = emit_content_chunk(&st.model, content, st.kind);
                        return Some((Ok(Bytes::from(out)), st));
                    }
                }

                // Unrecognised SSE line — skip.
                continue;
            }

            // Need more data from upstream.
            match st.stream.next().await {
                Some(Ok(chunk)) => {
                    st.buf.extend_from_slice(&chunk);
                }
                Some(Err(e)) => {
                    warn!("Upstream stream error: {e}");
                    st.done = true;
                    return Some((Err(std::io::Error::other(e)), st));
                }
                None => {
                    // Stream ended without [DONE] — emit final done chunk.
                    if !st.done {
                        st.done = true;
                        let out = emit_final_chunk(
                            &st.model,
                            st.start,
                            st.completion_tokens.unwrap_or(st.eval_count),
                            st.prompt_tokens.unwrap_or(0),
                            st.kind,
                        );
                        return Some((Ok(Bytes::from(out)), st));
                    }
                    return None;
                }
            }
        }
    })
}

/// Emit a single content chunk as NDJSON (done=false).
fn emit_content_chunk(model: &str, content: &str, kind: StreamKind) -> String {
    let json = match kind {
        StreamKind::Chat => serde_json::to_string(&OllamaChatStreamChunk {
            model: model.to_string(),
            created_at: now_rfc3339(),
            message: OllamaChatMessage {
                role: "assistant".to_string(),
                content: content.to_string(),
                images: None,
                tool_calls: None,
            },
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        }),
        StreamKind::Generate => serde_json::to_string(&OllamaGenerateStreamChunk {
            model: model.to_string(),
            created_at: now_rfc3339(),
            response: content.to_string(),
            done: false,
            done_reason: None,
            total_duration: None,
            load_duration: None,
            prompt_eval_count: None,
            prompt_eval_duration: None,
            eval_count: None,
            eval_duration: None,
        }),
    };

    let mut out = json.unwrap_or_default();
    out.push('\n');
    out
}

/// Emit the final done=true chunk with timing stats.
///
/// Timing breakdown is synthetic — llama-server does not expose per-phase
/// timing through the OpenAI API. `load_duration` is always 0 because the
/// model is pre-loaded by the runtime. The prompt/eval duration split is
/// an approximation (25%/75% of wall-clock time).
fn emit_final_chunk(
    model: &str,
    start: Instant,
    eval_count: u32,
    prompt_eval_count: u32,
    kind: StreamKind,
) -> String {
    let total_nanos = elapsed_nanos(start);

    let json = match kind {
        StreamKind::Chat => serde_json::to_string(&OllamaChatStreamChunk {
            model: model.to_string(),
            created_at: now_rfc3339(),
            message: OllamaChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                images: None,
                tool_calls: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: Some(total_nanos),
            load_duration: Some(0),
            prompt_eval_count: Some(prompt_eval_count),
            prompt_eval_duration: Some(total_nanos / 4),
            eval_count: Some(eval_count),
            eval_duration: Some(total_nanos * 3 / 4),
        }),
        StreamKind::Generate => serde_json::to_string(&OllamaGenerateStreamChunk {
            model: model.to_string(),
            created_at: now_rfc3339(),
            response: String::new(),
            done: true,
            done_reason: Some("stop".to_string()),
            total_duration: Some(total_nanos),
            load_duration: Some(0),
            prompt_eval_count: Some(prompt_eval_count),
            prompt_eval_duration: Some(total_nanos / 4),
            eval_count: Some(eval_count),
            eval_duration: Some(total_nanos * 3 / 4),
        }),
    };

    let mut out = json.unwrap_or_default();
    out.push('\n');
    out
}

/// Find the next newline in the buffer, returning the position after it.
fn find_newline(buf: &BytesMut) -> Option<usize> {
    buf.iter().position(|&b| b == b'\n').map(|pos| pos + 1)
}
