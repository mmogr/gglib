//! Integration tests for the `/api/voice/*` HTTP endpoints.
//!
//! These tests verify:
//!  - Every one of the 13 voice REST routes is wired correctly (no 404/405).
//!  - The JSON shape returned by `GET` endpoints matches the TypeScript types
//!    consumed by the frontend (`VoiceStatusResponse`, `VoiceModelsResponse`,
//!    `AudioDeviceInfo[]`).
//!  - Config mutations (`set_mode`, `set_voice`, `set_speed`, `set_auto_speak`)
//!    return 200 before any pipeline is started (persisted via `PendingConfig`).
//!  - `unload` is idempotent when no pipeline is active.
//!
//! Download and load operations (`download_stt_model`, `load_stt`, etc.)
//! may fail in the test environment (no model files on disk / no network),
//! but they must not return 404 or 405 — the route must exist and accept the
//! correct HTTP method and content type.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::ports::TEST_BASE_PORT;
use gglib_axum::bootstrap::{CorsConfig, ServerConfig, bootstrap};
use gglib_axum::routes::create_router;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_config() -> ServerConfig {
    ServerConfig {
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent: 1,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    }
}

/// Assert the response body is valid JSON and return the parsed value.
async fn parse_json(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap_or_else(|e| panic!("Expected valid JSON body: {e}"))
}

/// Assert a response has `application/json` content-type.
fn assert_json_content_type(response: &axum::response::Response) {
    let ct = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        ct.starts_with("application/json"),
        "Expected application/json content-type, got: {ct}"
    );
}

// ── GET /api/voice/status ─────────────────────────────────────────────────────

/// Route exists and returns JSON.
#[tokio::test]
async fn voice_status_returns_200_json() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/voice/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

/// The JSON body contains the fields the frontend `VoiceStatusResponse` type
/// expects (`isActive`, `state`, `mode`, `sttLoaded`, `ttsLoaded`,
/// `sttModelId`, `ttsVoice`, `autoSpeak`).
#[tokio::test]
async fn voice_status_json_shape_matches_frontend_type() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/voice/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = parse_json(response).await;

    // All fields required by the TS VoiceStatusResponse interface must be present.
    for field in &[
        "isActive",
        "state",
        "mode",
        "sttLoaded",
        "ttsLoaded",
        "autoSpeak",
    ] {
        assert!(
            json.get(field).is_some(),
            "voice/status response missing required field '{field}'. Got: {json}"
        );
    }
    // Nullable fields must at minimum be present (null or string).
    for field in &["sttModelId", "ttsVoice"] {
        assert!(
            json.get(field).is_some(),
            "voice/status response missing nullable field '{field}'. Got: {json}"
        );
    }

    // Type checks on the boolean fields.
    assert!(
        json["isActive"].is_boolean(),
        "isActive should be boolean, got: {}",
        json["isActive"]
    );
    assert!(
        json["sttLoaded"].is_boolean(),
        "sttLoaded should be boolean, got: {}",
        json["sttLoaded"]
    );
    assert!(
        json["ttsLoaded"].is_boolean(),
        "ttsLoaded should be boolean, got: {}",
        json["ttsLoaded"]
    );
    assert!(
        json["autoSpeak"].is_boolean(),
        "autoSpeak should be boolean, got: {}",
        json["autoSpeak"]
    );
}

// ── GET /api/voice/models ─────────────────────────────────────────────────────

/// Route exists and returns JSON.
#[tokio::test]
async fn voice_models_returns_200_json() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/voice/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

/// The JSON body contains the fields the frontend `VoiceModelsResponse` type
/// expects (`sttModels`, `ttsModel`, `voices`, `vadDownloaded`).
#[tokio::test]
async fn voice_models_json_shape_matches_frontend_type() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/voice/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = parse_json(response).await;

    // Top-level fields required by VoiceModelsResponse.
    for field in &["sttModels", "ttsModel", "voices", "vadDownloaded"] {
        assert!(
            json.get(field).is_some(),
            "voice/models response missing required field '{field}'. Got: {json}"
        );
    }
    assert!(json["sttModels"].is_array(), "sttModels should be an array");
    assert!(json["voices"].is_array(), "voices should be an array");
    assert!(json["ttsModel"].is_object(), "ttsModel should be an object");
    assert!(
        json["vadDownloaded"].is_boolean(),
        "vadDownloaded should be boolean"
    );

    // ttsModel must include isDownloaded (consumed by VoiceSettings.tsx).
    let tts_model = &json["ttsModel"];
    assert!(
        tts_model.get("isDownloaded").is_some(),
        "ttsModel missing 'isDownloaded'. Got: {tts_model}"
    );

    // Each SttModelInfo must include isDownloaded.
    if let Some(stt_models) = json["sttModels"].as_array() {
        for (i, model) in stt_models.iter().enumerate() {
            assert!(
                model.get("isDownloaded").is_some(),
                "sttModels[{i}] missing 'isDownloaded'. Got: {model}"
            );
            // Other required fields.
            for field in &[
                "id",
                "name",
                "sizeBytes",
                "sizeDisplay",
                "quality",
                "speed",
                "isDefault",
            ] {
                assert!(
                    model.get(*field).is_some(),
                    "sttModels[{i}] missing '{field}'. Got: {model}"
                );
            }
        }
    }
}

// ── GET /api/voice/devices ────────────────────────────────────────────────────

/// Route exists and returns a JSON array.  Device enumeration may return an
/// empty list in headless CI but must not error.
#[tokio::test]
async fn voice_devices_returns_200_json_array() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/voice/devices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // May return 200 (list, possibly empty) or 500 (no audio backend in CI).
    // What it must NOT do is return 404 (route missing) or 405 (wrong method).
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "GET /api/voice/devices route must exist"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /api/voice/devices must accept GET"
    );

    if response.status() == StatusCode::OK {
        assert_json_content_type(&response);
        let json = parse_json(response).await;
        assert!(json.is_array(), "voice/devices should return a JSON array");
    }
}

// ── POST /api/voice/unload ────────────────────────────────────────────────────

/// Unload is idempotent — returns 200 even when no pipeline is active.
#[tokio::test]
async fn voice_unload_is_idempotent() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/unload")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "POST /api/voice/unload should be idempotent (200 when no pipeline)"
    );
    assert_json_content_type(&response);
}

// ── PUT /api/voice/mode ───────────────────────────────────────────────────────

/// Mode "ptt" is persisted before any pipeline exists — must return 200.
#[tokio::test]
async fn voice_set_mode_ptt_returns_200() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"ptt"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

/// Mode "vad" is also persisted before any pipeline exists — must return 200.
#[tokio::test]
async fn voice_set_mode_vad_returns_200() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"vad"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

/// An unknown mode returns a JSON error, not a routing 404/405.
/// (VoicePipelinePort::set_mode maps unknown modes to VoicePortError::NotFound
/// which becomes HTTP 404 — the key test is that the handler ran, not routing.)
#[tokio::test]
async fn voice_set_mode_invalid_returns_json_error_not_405() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/mode")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"unknown_mode"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Route must exist and accept PUT.
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "PUT must be accepted on /api/voice/mode"
    );
    // Handler ran — response must be JSON (not a plain-text routing 404).
    assert_json_content_type(&response);
}

// ── PUT /api/voice/voice ──────────────────────────────────────────────────────

/// Setting a TTS voice is persisted before any pipeline — must return 200.
#[tokio::test]
async fn voice_set_voice_returns_200() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/voice")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"voiceId":"af_sarah"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

// ── PUT /api/voice/speed ──────────────────────────────────────────────────────

/// Setting TTS speed is persisted before any pipeline — must return 200.
#[tokio::test]
async fn voice_set_speed_returns_200() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/speed")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"speed":1.25}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

// ── PUT /api/voice/auto-speak ─────────────────────────────────────────────────

/// Setting auto-speak is persisted before any pipeline — must return 200.
#[tokio::test]
async fn voice_set_auto_speak_returns_200() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/voice/auto-speak")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"autoSpeak":false}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_json_content_type(&response);
}

// ── POST model download routes ────────────────────────────────────────────────
// These may fail in CI (no real model files / no network), but the routes
// must exist and accept POST with the correct shape.

#[tokio::test]
async fn voice_download_stt_route_exists_and_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/models/stt/base.en/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/models/stt/{{id}}/download route must exist"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be accepted on /api/voice/models/stt/{{id}}/download"
    );
}

#[tokio::test]
async fn voice_download_tts_route_exists_and_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/models/tts/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/models/tts/download route must exist"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be accepted on /api/voice/models/tts/download"
    );
}

#[tokio::test]
async fn voice_download_vad_route_exists_and_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/models/vad/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/models/vad/download route must exist"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be accepted on /api/voice/models/vad/download"
    );
}

// ── POST model load routes ────────────────────────────────────────────────────

#[tokio::test]
async fn voice_load_stt_route_exists_and_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/stt/load")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"modelId":"base.en"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Route must accept POST (not 405).
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be accepted on /api/voice/stt/load"
    );
    // Handler ran — response is JSON (model not available in test env yields 4xx,
    // but it must be JSON, not plain-text from a routing miss).
    assert_json_content_type(&response);
}

#[tokio::test]
async fn voice_load_tts_route_exists_and_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/tts/load")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Route must accept POST (not 405).
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be accepted on /api/voice/tts/load"
    );
    // Handler ran — response is JSON (TTS model not available in test env yields
    // 4xx/5xx, but it must be JSON, not plain-text from a routing miss).
    assert_json_content_type(&response);
}

// ── SPA fallback regression guard ────────────────────────────────────────────

/// Voice routes under /api/voice/* must not be intercepted by the SPA
/// fallback (would return HTML instead of JSON).
#[tokio::test]
async fn voice_routes_not_intercepted_by_spa_fallback() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    for uri in &[
        "/api/voice/status",
        "/api/voice/models",
        "/api/voice/devices",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(*uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let ct = response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or(""))
            .unwrap_or("");

        assert!(
            !ct.contains("text/html"),
            "{uri} was intercepted by SPA fallback (returned HTML). \
             Voice API routes must be matched before the SPA fallback."
        );
    }
}

// ── POST /api/voice/start ─────────────────────────────────────────────────────

/// Route exists and is wired to POST (not 404 / 405).
#[tokio::test]
async fn voice_start_route_exists() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/start")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"mode":"ptt"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/start must be routed"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST must be the correct method"
    );
}

/// A null body (from the frontend when no mode is passed) must NOT return 422.
/// Verifies that the extractor is Json<Option<StartRequest>>, not Option<Json<StartRequest>>.
#[tokio::test]
async fn voice_start_null_body_is_not_unprocessable() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/start")
                .header("content-type", "application/json")
                .body(Body::from("null"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "null body must not return 422 — frontend sends null when no mode is passed"
    );
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

// ── POST /api/voice/stop ──────────────────────────────────────────────────────

/// stop is idempotent when no pipeline is active — must return 204.
#[tokio::test]
async fn voice_stop_is_idempotent_returns_204() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "POST /api/voice/stop should be idempotent (204 with no active pipeline)"
    );
}

// ── POST /api/voice/ptt-start ─────────────────────────────────────────────────

/// Route exists and is wired to POST (not 404 / 405).
#[tokio::test]
async fn voice_ptt_start_route_exists() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/ptt-start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/ptt-start must be routed"
    );
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

// ── POST /api/voice/ptt-stop ──────────────────────────────────────────────────

/// Route exists; returns JSON (the transcript body) or an error, but never 404/405.
#[tokio::test]
async fn voice_ptt_stop_route_exists() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/ptt-stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "POST /api/voice/ptt-stop must be routed"
    );
    assert_ne!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

// ── POST /api/voice/speak ─────────────────────────────────────────────────────

/// speak always returns 202 Accepted (fire-and-forget at the HTTP layer).
/// The handler spawns a background task and returns immediately.
#[tokio::test]
async fn voice_speak_returns_202_accepted() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/speak")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"text":"hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "POST /api/voice/speak must always return 202 Accepted (synthesis is fire-and-forget)"
    );
}

// ── POST /api/voice/stop-speaking ────────────────────────────────────────────

/// stop-speaking is idempotent when nothing is playing — must return 204.
#[tokio::test]
async fn voice_stop_speaking_is_idempotent_returns_204() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/voice/stop-speaking")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "POST /api/voice/stop-speaking should be idempotent (204 when nothing is playing)"
    );
}
