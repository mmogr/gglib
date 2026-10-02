//! What the remote calls put on the wire to the daemon.

use super::DaemonHandle;

fn handle() -> DaemonHandle {
    DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: Some("the-token".to_owned()),
    }
}

/// A name holding `/` or a `:profile` suffix is one segment on the way to
/// the daemon, never a path of its own, and the context travels in the body.
#[test]
fn a_load_posts_the_identifier_as_one_segment_with_its_context() {
    let request = handle()
        .paired_load_request("org/qwen3:coding", Some(8192))
        .build()
        .expect("a request");

    assert_eq!(request.method(), reqwest::Method::POST);
    assert_eq!(
        request.url().path(),
        "/api/remote/models/org%2Fqwen3%3Acoding/load"
    );
    let auth = request.headers().get(reqwest::header::AUTHORIZATION);
    assert_eq!(auth.and_then(|v| v.to_str().ok()), Some("Bearer the-token"));
    let body: serde_json::Value = serde_json::from_slice(
        request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .expect("a buffered body"),
    )
    .unwrap();
    assert_eq!(body, serde_json::json!({ "num_ctx": 8192 }));
}

/// No `--ctx-size` is a `null` context: the model loads at the context it
/// would be served with there.
#[test]
fn a_load_without_a_context_leaves_it_to_the_far_machine() {
    let request = handle()
        .paired_load_request("3", None)
        .build()
        .expect("a request");

    assert_eq!(request.url().path(), "/api/remote/models/3/load");
    let body: serde_json::Value = serde_json::from_slice(
        request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .expect("a buffered body"),
    )
    .unwrap();
    assert_eq!(body, serde_json::json!({ "num_ctx": null }));
}
