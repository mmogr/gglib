//! What a client is sent for llama-server's non-2xx answer to a request that
//! does not stream.

use serde_json::json;

use super::*;

/// llama-server's number is named `upstream_error`, as a streamed reply's
/// is, and the rest of its object is kept.
#[test]
fn llama_servers_numeric_code_is_named_and_the_rest_is_kept() {
    let theirs = json!({ "error": {
        "code": 400, "message": "too long", "type": "exceed_context_size_error",
        "n_prompt_tokens": 9000, "n_ctx": 4096
    } });
    let body = relayed_error_body(StatusCode::BAD_REQUEST, theirs.to_string().as_bytes());
    assert_eq!(
        body,
        json!({ "error": {
            "code": "upstream_error", "message": "too long", "type": "exceed_context_size_error",
            "n_prompt_tokens": 9000, "n_ctx": 4096
        } })
    );
}

/// A code that is already a string is the upstream's to keep.
#[test]
fn a_string_code_is_kept() {
    let theirs = json!({ "error": {
        "code": "model_loading", "message": "Loading model", "type": "unavailable_error"
    } });
    let body = relayed_error_body(
        StatusCode::SERVICE_UNAVAILABLE,
        theirs.to_string().as_bytes(),
    );
    assert_eq!(body, theirs);
}

/// An `error` with no message or type, or one that is plain text, still
/// leaves as the whole object.
#[test]
fn what_the_upstream_left_out_is_filled_in() {
    let bare = relayed_error_body(StatusCode::BAD_GATEWAY, br#"{"error":{"code":502}}"#);
    assert_eq!(
        bare,
        json!({ "error": {
            "code": "upstream_error", "message": "upstream returned an error", "type": "server_error"
        } })
    );
    let text = relayed_error_body(StatusCode::BAD_GATEWAY, br#"{"error":"gone"}"#);
    assert_eq!(
        text,
        json!({ "error": { "code": "upstream_error", "message": "gone", "type": "server_error" } })
    );
}

/// A body that is not the error object, JSON or not, becomes one around the
/// bytes that came.
#[test]
fn a_body_that_is_no_error_object_becomes_one() {
    for raw in ["<html>Bad Gateway</html>", r#"{"detail":"no"}"#, ""] {
        let body = relayed_error_body(StatusCode::BAD_GATEWAY, raw.as_bytes());
        assert_eq!(
            body,
            json!({ "error": {
                "code": "upstream_error",
                "message": format!("upstream returned 502 Bad Gateway: {raw}"),
                "type": "server_error"
            } }),
            "{raw:?}"
        );
    }
}
