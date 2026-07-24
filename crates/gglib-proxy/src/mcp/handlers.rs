//! Axum route handlers for the MCP Streamable HTTP transport.
//!
//! Implements the single-endpoint design from MCP spec 2025-03-26:
//!
//! | Method   | Path   | Behaviour                                       |
//! |----------|--------|-------------------------------------------------|
//! | `POST`   | `/mcp` | JSON-RPC dispatch (initialize, tools/*, ping)   |
//! | `GET`    | `/mcp` | 405 Method Not Allowed (no server-push yet)     |
//! | `DELETE` | `/mcp` | Session termination via `Mcp-Session-Id` header |
//!
//! # Progressive Disclosure
//!
//! `tools/list` no longer exposes raw tool schemas. Instead it returns
//! exactly **three meta-tools** (`search_tools`, `get_tool_schema`,
//! `invoke_tool`). External clients discover capabilities incrementally
//! rather than receiving every schema up-front, reducing context-window
//! consumption by 90%+. See [`super::meta_tools`] for details.
//!
//! `tools/call` results are returned as `text/event-stream` (SSE).
//! All other methods return `application/json`. Both are valid per spec.

use std::collections::HashMap;
use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use serde_json::Value;
use tracing::{debug, error, warn};

use crate::server::AppState;

use super::types::*;

/// Header name for the MCP session ID.
const MCP_SESSION_HEADER: &str = "mcp-session-id";

// ─── POST /mcp ─────────────────────────────────────────────────────────────

/// Main MCP Streamable HTTP handler.
///
/// Parses the JSON-RPC request, validates the Origin header, and
/// dispatches to the appropriate method handler.
pub(crate) async fn post_mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Validate Origin header (spec §Security Warning)
    if let Err(resp) = validate_origin(&headers) {
        return resp;
    }

    // Parse JSON-RPC request
    let request: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            warn!("MCP: invalid JSON-RPC body: {e}");
            return json_rpc_error_response(
                StatusCode::BAD_REQUEST,
                Value::Null,
                JsonRpcError::new(PARSE_ERROR, format!("Parse error: {e}")),
            );
        }
    };

    debug!(method = %request.method, id = ?request.id, "MCP request");

    // Notification (no id) — return 202 Accepted
    if request.id.is_none() {
        return handle_notification(&state.sessions, &headers, &request).await;
    }

    let id = request.id.unwrap();

    // Session validation for non-initialize requests
    if request.method != "initialize"
        && let Err(resp) = require_session(&state.sessions, &headers, &id).await
    {
        return resp;
    }

    // Dispatch by method
    match request.method.as_str() {
        "initialize" => handle_initialize(&state.sessions, id).await,
        "ping" => handle_ping(id),
        "tools/list" => handle_meta_tools_list(&state.mcp, id).await,
        "tools/call" => handle_meta_tools_call(&state.mcp, id, request.params).await,
        _ => json_rpc_error_response(
            StatusCode::OK,
            id,
            JsonRpcError::new(METHOD_NOT_FOUND, "Method not found"),
        ),
    }
}

// ─── GET /mcp ──────────────────────────────────────────────────────────────

/// Returns 405 Method Not Allowed.
///
/// The spec permits this when the server does not offer a server-initiated
/// SSE stream. Server-push notifications are a future enhancement.
pub(crate) async fn get_mcp() -> impl IntoResponse {
    StatusCode::METHOD_NOT_ALLOWED
}

// ─── DELETE /mcp ───────────────────────────────────────────────────────────

/// Terminate the session identified by `Mcp-Session-Id`.
pub(crate) async fn delete_mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session_id = match headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(id) => id,
        None => return StatusCode::BAD_REQUEST,
    };

    if state.sessions.remove_session(session_id).await {
        debug!(session_id, "MCP session terminated");
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// ─── Method handlers ───────────────────────────────────────────────────────

/// Handle `initialize` — create session, return capabilities.
async fn handle_initialize(sessions: &super::session::SessionManager, id: Value) -> Response {
    let session_id = sessions.create_session().await;

    let result = InitializeResult {
        protocol_version: "2025-03-26",
        capabilities: ServerCapabilities {
            tools: Some(ToolCapabilities {
                list_changed: false,
            }),
        },
        server_info: ServerInfo {
            name: "gglib",
            version: env!("CARGO_PKG_VERSION"),
        },
    };

    let body = JsonRpcResponse::success(id, serde_json::to_value(result).unwrap());
    let mut resp = Json(body).into_response();
    resp.headers_mut().insert(
        MCP_SESSION_HEADER,
        HeaderValue::from_str(&session_id).unwrap(),
    );
    resp
}

/// Handle `ping` — return empty result.
fn handle_ping(id: Value) -> Response {
    Json(JsonRpcResponse::success(id, serde_json::json!({}))).into_response()
}

/// Handle `tools/list` — return the three progressive-disclosure meta-tools.
///
/// External clients (VS Code Copilot, OpenWebUI, etc.) receive exactly three
/// stable tool specs rather than the full registry. This keeps the baseline
/// context cost constant regardless of how many MCP servers are running.
async fn handle_meta_tools_list(mcp: &gglib_mcp::McpService, id: Value) -> Response {
    let index = super::meta_tools::build_tool_index(mcp).await;
    let specs = super::meta_tools::meta_tools_list(&index);
    let result = ToolsListResult { tools: specs };
    Json(JsonRpcResponse::success(
        id,
        serde_json::to_value(result).unwrap(),
    ))
    .into_response()
}

/// Handle `tools/call` — dispatch to one of the three meta-tools.
///
/// Routing is strict: only `search_tools`, `get_tool_schema`, and
/// `invoke_tool` are accepted. Any other name — including direct calls to
/// raw `"server__tool"` identifiers — returns `METHOD_NOT_FOUND`. There is
/// no legacy passthrough.
async fn handle_meta_tools_call(
    mcp: &gglib_mcp::McpService,
    id: Value,
    params: Option<Value>,
) -> Response {
    let params_val = match params {
        Some(v) => v,
        None => {
            return json_rpc_error_response(
                StatusCode::OK,
                id,
                JsonRpcError::new(INVALID_PARAMS, "Missing params"),
            );
        }
    };

    let name = match params_val.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return json_rpc_error_response(
                StatusCode::OK,
                id,
                JsonRpcError::new(INVALID_PARAMS, "Missing 'name' field"),
            );
        }
    };

    // Shorthand: the "arguments" object sent by the MCP client for this call.
    let meta_args = params_val
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    match name.as_str() {
        // ── search_tools ──────────────────────────────────────────────────
        "search_tools" => {
            let query = meta_args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let index = super::meta_tools::build_tool_index(mcp).await;
            let summaries = index.search(query);
            let text = serde_json::to_string(&summaries).unwrap_or_default();
            sse_tool_result(id, text, false)
        }

        // ── get_tool_schema ───────────────────────────────────────────────
        "get_tool_schema" => {
            let tool_id = match meta_args.get("tool_id").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => {
                    return json_rpc_error_response(
                        StatusCode::OK,
                        id,
                        JsonRpcError::new(INVALID_PARAMS, "Missing 'tool_id' argument"),
                    );
                }
            };
            let index = super::meta_tools::build_tool_index(mcp).await;
            match index.get_schema(&tool_id) {
                Some(schema) => {
                    let text = serde_json::to_string(schema).unwrap_or_default();
                    sse_tool_result(id, text, false)
                }
                None => json_rpc_error_response(
                    StatusCode::OK,
                    id,
                    JsonRpcError::new(
                        INVALID_PARAMS,
                        format!(
                            "Unknown tool: '{tool_id}'. Use search_tools to discover available tool IDs."
                        ),
                    ),
                ),
            }
        }

        // ── invoke_tool ───────────────────────────────────────────────────
        "invoke_tool" => {
            let tool_id = match meta_args.get("tool_id").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => {
                    return json_rpc_error_response(
                        StatusCode::OK,
                        id,
                        JsonRpcError::new(INVALID_PARAMS, "Missing 'tool_id' argument"),
                    );
                }
            };

            let (server_id, bare_name) = match super::meta_tools::resolve_tool_name(mcp, &tool_id)
                .await
            {
                Some(pair) => pair,
                None => {
                    return json_rpc_error_response(
                        StatusCode::OK,
                        id,
                        JsonRpcError::new(
                            INVALID_PARAMS,
                            format!(
                                "Unknown tool: '{tool_id}'. Use search_tools to discover available tool IDs."
                            ),
                        ),
                    );
                }
            };

            let tool_args: HashMap<String, Value> = match meta_args.get("arguments").cloned() {
                Some(Value::Object(map)) => map.into_iter().collect(),
                Some(_) => {
                    return json_rpc_error_response(
                        StatusCode::OK,
                        id,
                        JsonRpcError::new(INVALID_PARAMS, "'arguments' must be a JSON object"),
                    );
                }
                None => HashMap::new(),
            };

            match mcp.call_tool(server_id, &bare_name, tool_args).await {
                Ok(tool_result) => {
                    let text = tool_result
                        .data
                        .map(|d| serde_json::to_string_pretty(&d).unwrap_or_default())
                        .unwrap_or_default();
                    sse_tool_result(id, text, !tool_result.success)
                }
                Err(e) => {
                    error!("MCP invoke_tool error for '{tool_id}': {e}");
                    sse_tool_result(id, e.to_string(), true)
                }
            }
        }

        // ── Hard break — no legacy passthrough ────────────────────────────
        _ => json_rpc_error_response(
            StatusCode::OK,
            id,
            JsonRpcError::new(
                METHOD_NOT_FOUND,
                format!("Unknown tool: '{name}'. Use search_tools to discover available tools."),
            ),
        ),
    }
}

// ─── Notification handling ─────────────────────────────────────────────────

/// Handle JSON-RPC notifications (requests without an id).
///
/// Returns 202 Accepted for all valid notifications.
async fn handle_notification(
    sessions: &super::session::SessionManager,
    headers: &HeaderMap,
    request: &JsonRpcRequest,
) -> Response {
    if request.method == "notifications/initialized"
        && let Some(sid) = headers
            .get(MCP_SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
    {
        sessions.mark_initialized(sid).await;
        debug!(session_id = sid, "MCP session marked initialized");
    }
    StatusCode::ACCEPTED.into_response()
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Validate the Origin header.
///
/// Per spec §Security Warning: servers MUST validate the Origin header.
/// We reject requests from browser origins that don't match localhost.
#[allow(clippy::result_large_err)]
fn validate_origin(headers: &HeaderMap) -> Result<(), Response> {
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok())
        && !gglib_core::is_local_origin(origin)
    {
        warn!(origin, "MCP: rejected request with disallowed Origin");
        return Err(StatusCode::FORBIDDEN.into_response());
    }
    // No Origin header = non-browser client (curl, OpenWebUI server-side) — allow
    Ok(())
}

/// Verify the request carries a valid `Mcp-Session-Id`.
async fn require_session(
    sessions: &super::session::SessionManager,
    headers: &HeaderMap,
    id: &Value,
) -> Result<(), Response> {
    match headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(sid) if sessions.validate_session(sid).await => Ok(()),
        Some(_) => {
            // Session expired or unknown → client must re-initialize
            Err(json_rpc_error_response(
                StatusCode::NOT_FOUND,
                id.clone(),
                JsonRpcError::new(INVALID_REQUEST, "Unknown or expired session"),
            ))
        }
        None => Err(json_rpc_error_response(
            StatusCode::BAD_REQUEST,
            id.clone(),
            JsonRpcError::new(INVALID_REQUEST, "Missing Mcp-Session-Id header"),
        )),
    }
}

/// Wrap `text` in a standard MCP `CallToolResult` and return it as a
/// single-event SSE stream.
///
/// `is_error` maps to `CallToolResult::is_error`; pass `true` when the
/// upstream tool reported a failure so that the MCP client can distinguish
/// application-level errors from successful (but empty) results.
fn sse_tool_result(id: Value, text: String, is_error: bool) -> Response {
    let call_result = CallToolResult {
        content: vec![ToolContent {
            content_type: "text".to_string(),
            text,
        }],
        is_error: if is_error { Some(true) } else { None },
    };
    let rpc_response = JsonRpcResponse::success(id, serde_json::to_value(call_result).unwrap());
    let payload = serde_json::to_string(&rpc_response).unwrap();
    let event_stream = stream::once(async move {
        Ok::<_, Infallible>(Event::default().event("message").data(payload))
    });
    Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Build a JSON-RPC error as an HTTP response.
fn json_rpc_error_response(status: StatusCode, id: Value, error: JsonRpcError) -> Response {
    (status, Json(JsonRpcResponse::error(id, error))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use http_body_util::BodyExt;

    #[test]
    fn validate_origin_allows_no_origin_header() {
        let headers = HeaderMap::new();
        assert!(validate_origin(&headers).is_ok());
    }

    #[test]
    fn validate_origin_allows_localhost() {
        for origin in [
            "http://localhost",
            "http://localhost:3000",
            "https://localhost:8443",
            "http://127.0.0.1:9887",
            "https://127.0.0.1",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("origin", HeaderValue::from_str(origin).unwrap());
            assert!(
                validate_origin(&headers).is_ok(),
                "expected {origin} to be allowed"
            );
        }
    }

    #[test]
    fn validate_origin_rejects_external_origins() {
        for origin in [
            "https://evil.example.com",
            "http://attacker.io",
            "https://192.168.1.1:8080",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("origin", HeaderValue::from_str(origin).unwrap());
            assert!(
                validate_origin(&headers).is_err(),
                "expected {origin} to be rejected"
            );
        }
    }

    #[tokio::test]
    async fn json_rpc_error_response_has_correct_structure() {
        let resp = json_rpc_error_response(
            StatusCode::BAD_REQUEST,
            Value::Number(42.into()),
            JsonRpcError::new(PARSE_ERROR, "bad input"),
        );
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let parsed: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["error"]["code"], -32700);
        assert_eq!(parsed["error"]["message"], "bad input");
        assert!(parsed.get("result").is_none());
    }

    #[tokio::test]
    async fn session_manager_require_session_rejects_missing_header() {
        let sessions = super::super::session::SessionManager::new();
        let headers = HeaderMap::new();
        let id = Value::Number(1.into());

        let result = require_session(&sessions, &headers, &id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn session_manager_require_session_rejects_unknown_session() {
        let sessions = super::super::session::SessionManager::new();
        let mut headers = HeaderMap::new();
        headers.insert("mcp-session-id", HeaderValue::from_static("unknown-id"));
        let id = Value::Number(1.into());

        let result = require_session(&sessions, &headers, &id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn session_manager_require_session_accepts_valid_session() {
        let sessions = super::super::session::SessionManager::new();
        let sid = sessions.create_session().await;
        let mut headers = HeaderMap::new();
        headers.insert("mcp-session-id", HeaderValue::from_str(&sid).unwrap());
        let id = Value::Number(1.into());

        let result = require_session(&sessions, &headers, &id).await;
        assert!(result.is_ok());
    }
}
