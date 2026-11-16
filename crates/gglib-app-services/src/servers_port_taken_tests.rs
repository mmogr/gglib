//! A load whose endpoint cannot come on because another program holds its
//! port (#2047): the model is loaded all the same, since loading a model and
//! serving the endpoint are two things, and the answer carries why the
//! endpoint is off in the status's own sentence, so whoever asked can say it
//! beside the load's success.

use super::*;

/// The endpoint's port is held: the model is admitted, and the answer's
/// `endpoint_off` is the sentence the status carries, word for word.
#[tokio::test]
async fn a_load_goes_on_when_the_endpoints_port_is_taken() {
    let fx = Fixture::new(None, None).await;
    let _held = tokio::net::TcpListener::bind(("127.0.0.1", fx.proxy_port))
        .await
        .expect("the endpoint's port, held as another program would");

    let started = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    let why = fx.proxy.refused();

    let started = started.expect("the model loads though the endpoint did not come on");
    let answer = serde_json::to_value(&started).expect("the answer serializes");
    assert_eq!(answer["port"], fx.model_port, "{answer}");
    let why = why.expect("the status says why the endpoint is off");
    assert!(
        why.starts_with(&format!(
            "The endpoint is off: port {} is in use by another program.",
            fx.proxy_port
        )),
        "{why}"
    );
    assert_eq!(answer["endpoint_off"], why, "{answer}");
}

/// With the endpoint on, the answer is as it was: no `endpoint_off` at all,
/// so an older reader sees the shape it always did.
#[tokio::test]
async fn a_load_with_the_endpoint_on_says_nothing_of_it() {
    let fx = Fixture::new(None, None).await;

    let started = fx
        .ops
        .start(fx.model_id, StartServerRequest::default())
        .await;
    fx.proxy
        .stop()
        .await
        .expect("the proxy the start brought up");

    let answer = serde_json::to_value(started.expect("the start succeeds")).expect("serializes");
    assert!(answer.get("endpoint_off").is_none(), "{answer}");
}
