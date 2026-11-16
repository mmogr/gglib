//! A load whose endpoint could not come on (#2047): the model is loaded, so
//! the command succeeds, and why the endpoint is off is printed after the
//! load's own sentence, in the daemon's words. A pinned load exists to serve
//! the endpoint, so for it a refused endpoint is still the failure.

use super::*;

/// The status's sentence, as the daemon sends it beside a load.
const OFF: &str = "The endpoint is off: port 8080 is in use by another program. Stop it, \
                   or choose another port (Settings \u{203a} Endpoint, or `gglib config set \
                   proxy-port`), then turn the endpoint on.";

/// The load's sentence first, then why the endpoint is off: one request,
/// and success.
#[tokio::test]
async fn a_load_with_the_endpoint_off_prints_why_after_its_success() {
    let reply = serde_json::json!({
        "port": 9301,
        "message": "loaded at 9301 (stand-in)",
        "endpoint_off": OFF,
    });
    let (lines, taken) = against(vec![(200, reply.to_string())], |handle| async move {
        load_on(&handle, &body(3, None), false, &mut consent(false, false)).await
    })
    .await;

    assert_eq!(
        lines.expect("the model loaded"),
        ["  loaded at 9301 (stand-in)".to_owned(), format!("  {OFF}")]
    );
    assert_eq!(taken.len(), 1);
}

/// A pinned load whose endpoint the daemon refuses loads nothing: the
/// refusal is the error, and no start is sent after it.
#[tokio::test]
async fn a_pinned_load_whose_endpoint_is_refused_loads_nothing() {
    let refusal = serde_json::json!({ "error": OFF, "status": 409, "type": "conflict" });
    let (lines, taken) = against(vec![(409, refusal.to_string())], |handle| async move {
        load_on(&handle, &body(3, None), true, &mut consent(false, false)).await
    })
    .await;

    let err = format!("{:#}", lines.expect_err("the pinned endpoint was refused"));
    assert!(err.contains("port 8080 is in use"), "{err}");
    let sent: Vec<&str> = taken.iter().map(|t| t.line.as_str()).collect();
    assert_eq!(
        sent,
        [format!("POST {} HTTP/1.1", paths::PROXY_START_PINNED_PATH)]
    );
}
