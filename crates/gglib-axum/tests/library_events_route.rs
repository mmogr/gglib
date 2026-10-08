//! A library change made outside the daemon, put on its event stream:
//! `POST /api/events`.
//!
//! `gglib model update` in a terminal changes the library in its own
//! process, through the `ModelOps` the daemon's routes run, and posts the
//! event that emitted there. These read the stream as the app reads it.

mod common;

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use common::harness::test_state_and_app;
use common::origin::{HOST, authed, call};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon;
use gglib_core::events::AppEvent;

/// Add a model named `name` to the context's library, returning its id.
async fn catalogued(state: &gglib_axum::AxumContext, name: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// The daemon's event stream, held open as the app holds it.
async fn stream(app: &Router) -> Body {
    let request = authed().uri(daemon::EVENTS_PATH).header("host", HOST);
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body()
}

/// The next frame on `stream`, as the text it is sent as.
async fn next_frame(stream: &mut Body) -> String {
    let frame = tokio::time::timeout(Duration::from_secs(5), stream.frame())
        .await
        .expect("an event is on the stream")
        .expect("the stream is open")
        .unwrap();
    String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap()
}

/// The frame the stream carries for `event`.
fn frame_of(event: &AppEvent) -> String {
    format!("data: {}\n\n", serde_json::to_string(event).unwrap())
}

/// Post `event` as a command posts it: the event itself, as JSON.
async fn post(app: &Router, event: &AppEvent) {
    let body = serde_json::to_value(event).unwrap();
    let (status, answer) = call(app, Method::POST, daemon::EVENTS_PATH, Some(body)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{answer}");
}

/// An edit and a removal made from the app each put a frame on the stream.
/// The event a command posts for the same change, the one `ModelOps` emits
/// for the row it stored, arrives as that same frame.
#[tokio::test]
async fn an_event_a_command_posts_reaches_the_stream_as_the_apps_own_change_does() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = catalogued(&state, "qwen").await;
    let model = format!("{}/{id}", daemon::MODELS_LIST_PATH);
    let mut events = stream(&app).await;

    let renamed = Some(json!({ "name": "Renamed" }));
    let (status, answer) = call(&app, Method::PUT, &model, renamed).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let stored = state.core.models().get_by_id(id).await.unwrap().unwrap();
    assert_eq!(stored.name, "Renamed");
    let updated = AppEvent::model_updated((&stored).into());
    assert_eq!(next_frame(&mut events).await, frame_of(&updated));

    post(&app, &updated).await;
    assert_eq!(next_frame(&mut events).await, frame_of(&updated));

    let unforced = Some(json!({ "force": false }));
    let (status, answer) = call(&app, Method::DELETE, &model, unforced).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let removed = AppEvent::model_removed(id);
    assert_eq!(next_frame(&mut events).await, frame_of(&removed));

    post(&app, &removed).await;
    assert_eq!(next_frame(&mut events).await, frame_of(&removed));
}
