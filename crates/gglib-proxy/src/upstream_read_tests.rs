//! Tests for [`super`]: the `upstream_timeout` bodies. The reply's reading
//! is tested in `upstream_frames_tests.rs`.

use super::*;

/// Replace the value after `key` (the run of characters `keep` accepts) with
/// `mask`, asserting there is exactly one such value and that it is non-empty.
fn mask_one(body: &str, key: &str, keep: fn(char) -> bool, mask: &str) -> String {
    assert_eq!(body.matches(key).count(), 1, "expected one {key} in {body}");
    let start = body.find(key).expect("counted above") + key.len();
    let len: usize = body[start..]
        .chars()
        .take_while(|&c| keep(c))
        .map(char::len_utf8)
        .sum();
    assert!(len > 0, "{key} has no value in {body}");
    format!("{}{mask}{}", &body[..start], &body[start + len..])
}

/// The first-byte timeout body, byte for byte, except the chunk id, which is
/// random, and the `created` timestamp, which follows the clock. ggchat's
/// `ErrorTests` quote this error (code `upstream_timeout`, message "upstream
/// did not respond within 300s") and read it as "wait and retry"; any change
/// to these bytes is a change to what clients are sent.
#[test]
fn the_first_byte_timeout_body_is_a_notice_then_upstream_timeout_then_done() {
    let body = first_byte_timeout_frame("some-model", StreamBounds::default().first_byte);
    let body = mask_one(&body, "\"id\":\"chatcmpl-", |c| c.is_ascii_hexdigit(), "ID");
    let body = mask_one(&body, "\"created\":", |c| c.is_ascii_digit(), "0");
    assert_eq!(body, EXPECTED_FIRST_BYTE_TIMEOUT_BODY);
}

const EXPECTED_FIRST_BYTE_TIMEOUT_BODY: &str = concat!(
    r#"data: {"choices":[{"delta":{"content":"#,
    r#""⚠️ [proxy] upstream model server did not begin responding within 300s"#,
    r#" — it may be overloaded or wedged."#,
    r#" Retry; if it persists the model will be recycled."#,
    r#""},"finish_reason":null,"index":0}],"#,
    r#""created":0,"id":"chatcmpl-ID","#,
    r#""model":"some-model","object":"chat.completion.chunk"}"#,
    "\n\n",
    r#"data: {"error":{"code":"upstream_timeout","#,
    r#""message":"upstream did not respond within 300s","#,
    r#""type":"server_error"}}"#,
    "\n\n",
    "data: [DONE]\n\n",
);

/// What the client is sent for a stall: the notice, which promises a recycle
/// only when there will be one, and the error frame, byte for byte.
#[test]
fn a_stall_is_told_as_a_notice_and_an_upstream_timeout_frame() {
    let after = Duration::from_mins(5);
    let mid = UpstreamStalled {
        after,
        after_first_token: true,
    };
    let early = UpstreamStalled {
        after,
        after_first_token: false,
    };

    assert_eq!(
        mid.notice(),
        "\n\n⚠️ [proxy] upstream model server went silent for 300s mid-response — it may be wedged; this model is being recycled."
    );
    assert_eq!(
        early.notice(),
        "\n\n⚠️ [proxy] upstream model server went silent for 300s mid-response — it may be wedged."
    );
    assert_eq!(
        mid.error_frame(),
        concat!(
            r#"data: {"error":{"code":"upstream_timeout","#,
            r#""message":"upstream sent nothing for 300s mid-response","#,
            r#""type":"server_error"}}"#,
            "\n\n",
        )
    );
}
