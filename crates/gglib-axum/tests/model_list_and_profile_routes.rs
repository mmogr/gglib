//! Two routes the library page and the settings page call: the model list
//! narrowed by a context-length range, and the starter-profile install.

mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_state_and_app;
use common::origin::{HOST, authed};
use gglib_core::CorsConfig;

/// Send `method path` as the daemon's own page does, and read the JSON back.
async fn answer(app: &axum::Router, method: &str, path: &str) -> (StatusCode, serde_json::Value) {
    let request = authed().method(method).uri(path).header("Host", HOST);
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

/// The `name` of each object in a JSON array, in its order.
fn names(list: &serde_json::Value) -> Vec<&str> {
    let rows = list.as_array().expect("a JSON array");
    rows.iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect()
}

/// Add a model called `name` with a context window of `context` tokens.
async fn catalogued(state: &gglib_axum::AxumContext, name: &str, context: u64) {
    let mut model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    model.context_length = Some(context);
    state.core.models().add(model).await.unwrap();
}

/// What the library page's Context Length slider asks for: only the models
/// whose window is inside the range come back.
#[tokio::test]
async fn the_list_is_narrowed_by_the_context_range_it_is_asked_for() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    catalogued(&state, "small", 8_192).await;
    catalogued(&state, "middle", 32_768).await;
    catalogued(&state, "large", 131_072).await;

    let (status, all) = answer(&app, "GET", "/api/models?sort=name&order=asc").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&all), ["large", "middle", "small"]);

    let range = "/api/models?sort=name&order=asc&min_context=10000&max_context=40000";
    let (status, narrowed) = answer(&app, "GET", range).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&narrowed), ["middle"]);

    let (_, floor) = answer(
        &app,
        "GET",
        "/api/models?sort=name&order=asc&min_context=10000",
    )
    .await;
    assert_eq!(names(&floor), ["large", "middle"]);
    let (_, ceiling) = answer(
        &app,
        "GET",
        "/api/models?sort=name&order=asc&max_context=40000",
    )
    .await;
    assert_eq!(names(&ceiling), ["middle", "small"]);
}

/// The settings page's button: the nine starter profiles are stored, and the
/// answer carries them. A second press adds nothing and says what it kept.
#[tokio::test]
async fn the_install_route_stores_the_nine_starter_profiles() {
    const TEMPLATES: [&str; 9] = [
        "coding", "chat", "creative", "minimal", "low", "medium", "high", "xhigh", "max",
    ];
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let path = "/api/config/profiles/install-templates";

    let (status, done) = answer(&app, "POST", path).await;

    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["installed"], serde_json::json!(TEMPLATES));
    assert_eq!(done["kept"], serde_json::json!([]));
    assert_eq!(names(&done["settings"]["inferenceProfiles"]), TEMPLATES);
    let stored = state.core.settings().get().await.unwrap();
    assert_eq!(
        stored.inference_profiles,
        Some(gglib_core::domain::builtin_templates())
    );

    let (_, again) = answer(&app, "POST", path).await;
    assert_eq!(again["installed"], serde_json::json!([]));
    assert_eq!(again["kept"], serde_json::json!(TEMPLATES));
}
