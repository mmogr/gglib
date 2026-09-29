//! `/v1/runs/*` on the real proxy: the door a paired device reaches its own
//! runs through, and who the proxy takes each caller for.
//!
//! The runs are a stand-in (`fixtures::runs`) that records the scope of every
//! call, because what is under test is the door: the scope, the error shape,
//! the event framing and the shutdown. What each scope may see is the
//! registry's, tested where it lives.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_core::ports::{RunScope, RunsError, RunsPort};
use reqwest::{Client, StatusCode};

mod fixtures;
use fixtures::runs::{FakeRuns, code, info, json, routes, serve};
use fixtures::tunnel::{DEVICE, DEVICE_KEY, PROXY_KEY, get, spawn_proxy_serving, tunnel_to};

#[tokio::test]
async fn every_route_answers_503_with_a_code_when_the_proxy_holds_no_runs() {
    let (base, cancel) = serve(None).await;
    for request in routes(&base) {
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(code(&body), "runs_unavailable");
    }
    cancel.cancel();
}

#[tokio::test]
async fn a_local_client_is_served_in_the_local_scope_on_every_route() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    for request in routes(&base) {
        let status = request.send().await.unwrap().status();
        assert!(status.is_success(), "{status}");
    }
    assert_eq!(runs.scopes(), vec![RunScope::Local; 5]);
    cancel.cancel();
}

#[tokio::test]
async fn a_new_run_is_201_and_an_existing_one_200_with_the_run_as_the_body() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let put = || {
        Client::new()
            .put(format!("{base}/v1/runs/abc"))
            .json(&serde_json::json!({"messages": []}))
            .send()
    };

    let (status, body) = json(put().await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body, serde_json::to_value(info("abc")).unwrap());

    runs.existing.store(true, Ordering::SeqCst);
    assert_eq!(put().await.unwrap().status(), StatusCode::OK);
    cancel.cancel();
}

/// The device header alone is not a tunnelled request: a local client that
/// writes it is served exactly as one that does not, so it gains nothing.
#[tokio::test]
async fn the_device_header_alone_leaves_a_local_client_local() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    for request in routes(&base) {
        request
            .header("x-modelpipe-device", DEVICE)
            .send()
            .await
            .unwrap();
    }
    assert_eq!(runs.scopes(), vec![RunScope::Local; 5]);
    cancel.cancel();
}

/// The limit, pinned so it is not forgotten: a client that writes `Via` as
/// well is taken for the device it names. The proxy has nothing else to tell
/// a tunnelled request by, so "this machine may not read a device's reply"
/// is a courtesy of the API, not a boundary. A real discriminator added
/// later fails this test, which is the moment to update the docs that say so.
#[tokio::test]
async fn forged_markers_are_taken_for_the_device_they_name() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    Client::new()
        .get(format!("{base}/v1/runs"))
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-device", DEVICE)
        .send()
        .await
        .unwrap();
    assert_eq!(runs.scopes(), vec![RunScope::Device(DEVICE.to_owned())]);
    cancel.cancel();
}

#[tokio::test]
async fn a_tunnelled_request_that_names_no_device_reaches_no_run() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    for request in routes(&base) {
        let (status, body) =
            json(request.header("via", "1.1 modelpipe").send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(code(&body), "device_not_paired");
    }
    assert!(runs.scopes().is_empty());
    cancel.cancel();
}

#[tokio::test]
async fn every_refusal_carries_the_registry_code_in_the_proxy_error_shape() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    for err in [
        RunsError::InvalidId,
        RunsError::InvalidBody,
        RunsError::NotFound,
        RunsError::IdTaken,
        RunsError::NotYours,
        RunsError::TooManyRuns,
    ] {
        *runs.fail.lock().unwrap() = Some(err);
        let response = Client::new()
            .put(format!("{base}/v1/runs/r1"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        let (status, body) = json(response).await;
        assert_eq!(status.as_u16(), err.http_status(), "{err:?}");
        assert_eq!(code(&body), err.code(), "{err:?}");
        assert_eq!(body["error"]["message"], err.to_string(), "{err:?}");
    }
    cancel.cancel();
}

/// Nothing a client sent comes back in a refusal: not a body the route could
/// not read, not a query it could not parse.
#[tokio::test]
async fn a_refusal_echoes_nothing_the_client_sent() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let secret = "zzq-private-words";

    let response = Client::new()
        .put(format!("{base}/v1/runs/r1"))
        .header("content-type", "application/json")
        .body(format!("{{\"messages\": \"{secret}"))
        .send()
        .await
        .unwrap();
    let (status, body) = json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(code(&body), "invalid_request");
    assert!(!body.to_string().contains(secret), "{body}");

    let response = Client::new()
        .get(format!("{base}/v1/runs/r1/events?after={secret}"))
        .send()
        .await
        .unwrap();
    let (status, body) = json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(code(&body), "invalid_request");
    assert!(!body.to_string().contains(secret), "{body}");
    assert!(runs.scopes().is_empty(), "neither reached the runs");
    cancel.cancel();
}

/// The daemon's framing, byte for byte: the proxy shares it.
#[tokio::test]
async fn a_runs_events_are_the_daemons_frames_after_the_cursor() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let response = Client::new()
        .get(format!("{base}/v1/runs/r1/events?after=5"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    let end = serde_json::to_string(&info("r1")).unwrap();
    assert_eq!(
        body,
        format!("id: 1\ndata: {{\"a\":1}}\n\nevent: run\ndata: {end}\n\n")
    );
    assert_eq!(*runs.after.lock().unwrap(), Some(5));
    cancel.cancel();
}

/// A run still going holds its reader open; a graceful stop must not wait
/// on it.
#[tokio::test]
async fn a_runs_event_stream_ends_when_the_proxy_shuts_down() {
    let runs = Arc::new(FakeRuns::default());
    runs.endless.store(true, Ordering::SeqCst);
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let response = Client::new()
        .get(format!("{base}/v1/runs/r1/events"))
        .send()
        .await
        .unwrap();
    let mut body = response.bytes_stream();
    let first = body.next().await.unwrap().unwrap();
    assert!(first.starts_with(b"id: 1\n"), "{first:?}");

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(chunk) = body.next().await {
            chunk.ok();
        }
    })
    .await
    .expect("the stream ends when the proxy shuts down");
}

/// Through a real tunnel, a device is served in its own scope: the one the
/// edge named for the key it presented.
#[tokio::test]
async fn a_device_through_the_tunnel_is_served_in_its_own_scope() {
    let runs = Arc::new(FakeRuns::default());
    let (proxy_url, cancel, _) =
        spawn_proxy_serving(PROXY_KEY, Some(Arc::clone(&runs) as Arc<dyn RunsPort>)).await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    let answer = get(&format!("{base}/runs"), Some(DEVICE_KEY)).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(runs.scopes(), vec![RunScope::Device(DEVICE.to_owned())]);

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
