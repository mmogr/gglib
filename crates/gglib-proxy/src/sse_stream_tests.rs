//! Tests for [`super`]: what a streaming client is sent when the upstream
//! answers with an error status, or cannot be reached at all.
//!
//! Each body is a visible notice, the error frame and `[DONE]`. The notice
//! frame carries a random id and the clock, so it is read as JSON here; every
//! byte after it is pinned, because clients parse the error frame (ggchat's
//! `WireTests` quote this envelope, keys in this order).

use super::*;

/// The notice a person reads, and every byte that follows the notice frame.
fn notice_then_rest(body: &str) -> (String, &str) {
    let (notice, rest) = body
        .split_once("\n\n")
        .expect("a notice frame, then the rest");
    let notice = notice.strip_prefix("data: ").expect("a data frame");
    let notice: serde_json::Value = serde_json::from_str(notice).expect("the notice is JSON");
    assert_eq!(notice["object"], "chat.completion.chunk");
    assert_eq!(notice["model"], "some-model");
    let text = notice
        .pointer("/choices/0/delta/content")
        .and_then(serde_json::Value::as_str)
        .expect("the notice has text");
    (text.to_owned(), rest)
}

/// Everything a streaming client is sent for a request to `url`, through
/// [`spawn_and_return`] as the chat path calls it with the slot cache off.
async fn streamed_from(url: &str) -> String {
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let response = spawn_and_return(
        crate::loopback::client().post(url),
        Bytes::from_static(b"{}"),
        ClientSender::new(tx, crate::client_send::CLIENT_SEND_TIMEOUT),
        rx,
        registry.register("some-model", true, None),
        "some-model".to_owned(),
        None,
        Arc::new(UpstreamHealth::new()),
        StreamBounds::default(),
        Arc::new(TokenCalibration::new()),
        Arc::new(CacheMetricsStore::new()),
        Arc::new(crate::metrics::ContextMetricsStore::new()),
        0,
        None,
        None,
        None,
        None,
        None,
        RepairTurn::OFF,
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body ends");
    String::from_utf8(body.to_vec()).expect("the body is text")
}

/// llama-server's own `type` and `code` reach the client, so it can still tell
/// a full context from any other failure.
#[tokio::test]
async fn a_streamed_request_the_upstream_refuses_ends_in_its_own_error_and_done() {
    const REFUSAL: &str = concat!(
        r#"{"error":{"code":"context_length_exceeded","#,
        r#""message":"the request exceeds the available context size","#,
        r#""type":"exceed_context_size_error"}}"#,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let url = format!("http://{}/", listener.local_addr().expect("an address"));
    let app = axum::Router::new().fallback(|| async { (StatusCode::BAD_REQUEST, REFUSAL) });
    let upstream = tokio::spawn(async move { axum::serve(listener, app).await });

    let body = streamed_from(&url).await;
    upstream.abort();
    let (notice, rest) = notice_then_rest(&body);

    assert_eq!(
        notice,
        "⚠️ [proxy] upstream model server error (400 Bad Request): the request exceeds the available context size"
    );
    assert_eq!(
        rest,
        concat!(
            r#"data: {"error":{"code":"context_length_exceeded","#,
            r#""message":"the request exceeds the available context size","#,
            r#""type":"exceed_context_size_error"}}"#,
            "\n\n",
            "data: [DONE]\n\n",
        )
    );
}

/// Each field falls back on its own. llama-server writes `code` as a number,
/// which is not a code a client can match on; a JSON body with no `error` at
/// all names nothing.
#[test]
fn an_upstream_error_falls_back_field_by_field_on_what_it_did_not_name() {
    let numbered =
        br#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#;
    let body = upstream_error_body("some-model", StatusCode::SERVICE_UNAVAILABLE, numbered);
    let (notice, rest) = notice_then_rest(&body);
    assert_eq!(
        notice,
        "⚠️ [proxy] upstream model server error (503 Service Unavailable): Loading model"
    );
    assert_eq!(
        rest,
        concat!(
            r#"data: {"error":{"code":"upstream_error","#,
            r#""message":"Loading model","type":"unavailable_error"}}"#,
            "\n\n",
            "data: [DONE]\n\n",
        )
    );

    let nameless = br#"{"detail":"no"}"#;
    let body = upstream_error_body("some-model", StatusCode::INTERNAL_SERVER_ERROR, nameless);
    let (notice, rest) = notice_then_rest(&body);
    assert_eq!(
        notice,
        "⚠️ [proxy] upstream model server error (500 Internal Server Error): upstream returned an error"
    );
    assert_eq!(
        rest,
        concat!(
            r#"data: {"error":{"code":"upstream_error","#,
            r#""message":"upstream returned an error","type":"server_error"}}"#,
            "\n\n",
            "data: [DONE]\n\n",
        )
    );
}

/// A body that is not JSON is quoted whole, behind the status, and escaped as
/// JSON escapes it.
#[test]
fn an_upstream_error_that_is_not_json_is_wrapped_with_its_status_and_raw_body() {
    let html = b"<h1>502 \"Bad Gateway\"</h1>\n";
    let body = upstream_error_body("some-model", StatusCode::BAD_GATEWAY, html);
    let (notice, rest) = notice_then_rest(&body);

    assert_eq!(
        notice,
        "⚠️ [proxy] upstream model server error (502 Bad Gateway): upstream returned 502 Bad Gateway: <h1>502 \"Bad Gateway\"</h1>\n"
    );
    assert_eq!(
        rest,
        concat!(
            r#"data: {"error":{"code":"upstream_error","#,
            r#""message":"upstream returned 502 Bad Gateway: <h1>502 \"Bad Gateway\"</h1>\n","#,
            r#""type":"server_error"}}"#,
            "\n\n",
            "data: [DONE]\n\n",
        )
    );
}

#[test]
fn an_upstream_that_cannot_be_reached_is_told_as_upstream_error() {
    let body = unreachable_body("some-model", &"connection refused");
    let (notice, rest) = notice_then_rest(&body);

    assert_eq!(
        notice,
        "⚠️ [proxy] upstream llama-server unavailable: connection refused"
    );
    assert_eq!(
        rest,
        concat!(
            r#"data: {"error":{"code":"upstream_error","#,
            r#""message":"upstream llama-server unavailable: connection refused","#,
            r#""type":"server_error"}}"#,
            "\n\n",
            "data: [DONE]\n\n",
        )
    );
}

/// The send's own error is the reason, in the notice and in the frame alike.
/// Its wording is reqwest's, so it is read back from the notice, not quoted.
#[tokio::test]
async fn a_streamed_request_that_cannot_reach_the_upstream_ends_in_upstream_error_and_done() {
    // Bound and dropped at once: nothing listens there.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let url = format!("http://{}/", listener.local_addr().expect("an address"));
    drop(listener);

    let body = streamed_from(&url).await;
    let (notice, rest) = notice_then_rest(&body);

    let message = notice
        .strip_prefix("⚠️ [proxy] ")
        .expect("the notice is the proxy's");
    let reason = message
        .strip_prefix("upstream llama-server unavailable: ")
        .expect("the notice says the upstream is unavailable");
    assert!(!reason.is_empty(), "the send's error is the reason");
    let message = serde_json::to_string(message).expect("a JSON string");
    assert_eq!(
        rest,
        format!(
            "data: {{\"error\":{{\"code\":\"upstream_error\",\"message\":{message},\"type\":\"server_error\"}}}}\n\ndata: [DONE]\n\n"
        )
    );
}
