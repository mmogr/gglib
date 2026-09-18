//! The loop guard's log is readable over the daemon's API (#1052): the route
//! answers from the database, a day the guard scanned and never tripped
//! included, newest day first.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use gglib_core::contracts::http::daemon::PROXY_LOOP_GUARD_TRIPS_PATH;
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::loop_guard_log::LoopGuardTripEvent;
use gglib_core::ports::LoopGuardTripSink;
use gglib_core::{CorsConfig, LoopGuardMode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use common::harness::test_state_and_app;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .header("Host", "127.0.0.1:9887")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_route_answers_from_the_log_with_the_days_that_never_tripped() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let now = now_secs();
    let writer = &state.loop_guard_trip_writer;
    writer.record_scan("qwen", LoopGuardMode::Note, now);
    writer.record_scan("qwen", LoopGuardMode::Note, now);
    writer.record_trip(
        LoopGuardTripEvent::new(now, "qwen", LoopGuardTrip::Loop, LoopGuardMode::Note)
            .with_session("s1"),
    );
    writer.record_scan("gemma", LoopGuardMode::Refuse, now);
    writer.record_scan("qwen", LoopGuardMode::Note, now - 86_400);
    writer.shutdown().await;

    let (status, days) = get(app, &format!("{PROXY_LOOP_GUARD_TRIPS_PATH}?since_days=7")).await;

    assert_eq!(status, StatusCode::OK, "{days}");
    let rows = days.as_array().expect("an array of days");
    assert_eq!(rows.len(), 3, "{days}");
    assert_eq!(rows[2]["model_name"], "qwen", "yesterday comes after today");
    assert_eq!(
        (rows[2]["scanned"].as_u64(), rows[2]["trips"].as_u64()),
        (Some(1), Some(0))
    );
    assert!(rows[2]["day"].as_str() < rows[0]["day"].as_str(), "{days}");
    assert_eq!(rows[0]["model_name"], "gemma");
    assert_eq!(rows[0]["mode"], "refuse");
    assert_eq!(
        (rows[0]["scanned"].as_u64(), rows[0]["trips"].as_u64()),
        (Some(1), Some(0))
    );
    assert_eq!(rows[1]["model_name"], "qwen");
    assert_eq!(rows[1]["mode"], "note");
    assert_eq!(
        (rows[1]["scanned"].as_u64(), rows[1]["trips"].as_u64()),
        (Some(2), Some(1))
    );
    assert_eq!(rows[1]["loops"].as_u64(), Some(1));
    assert_eq!(rows[1]["sessions"].as_u64(), Some(1));
}

#[tokio::test]
async fn a_window_wider_than_the_log_keeps_is_answered_not_refused() {
    let (_state, app) = test_state_and_app(CorsConfig::AllowAll).await;

    let (status, days) = get(
        app,
        &format!("{PROXY_LOOP_GUARD_TRIPS_PATH}?since_days=100000"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{days}");
    assert_eq!(days, serde_json::json!([]));
}

#[tokio::test]
async fn the_window_is_the_one_asked_for_and_thirty_days_when_none_is() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let writer = &state.loop_guard_trip_writer;
    let now = now_secs();
    writer.record_scan("qwen", LoopGuardMode::Note, now - 10 * 86_400);
    // Forty days back: a day the log keeps, outside the default window.
    writer.record_scan("qwen", LoopGuardMode::Note, now - 40 * 86_400);
    writer.shutdown().await;

    let (_, week) = get(
        app.clone(),
        &format!("{PROXY_LOOP_GUARD_TRIPS_PATH}?since_days=7"),
    )
    .await;
    assert_eq!(
        week,
        serde_json::json!([]),
        "ten days ago is outside a week"
    );
    let (_, month) = get(
        app.clone(),
        &format!("{PROXY_LOOP_GUARD_TRIPS_PATH}?since_days=30"),
    )
    .await;
    assert_eq!(month.as_array().map(Vec::len), Some(1), "{month}");
    let (_, default) = get(app, PROXY_LOOP_GUARD_TRIPS_PATH).await;
    assert_eq!(
        default.as_array().map(Vec::len),
        Some(1),
        "no window asked for is thirty days: {default}"
    );
}
