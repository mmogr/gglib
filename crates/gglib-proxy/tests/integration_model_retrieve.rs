//! `GET /v1/models/{name}` answers the entry `/v1/models` lists under that
//! `id`, byte for byte, and nothing the list does not hold.
//!
//! The catalogue here has twins (listed under `@` ids), a name holding a `/`
//! (sent as `%2F`, one path segment), a listed profile (`:coding` entries)
//! and a running model (a live context window), so an entry built any other
//! way than the list builds it would differ somewhere.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::Value;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, PinnedSpec, RunningTarget,
};

use fixtures::common::{settings_listing, spawn_proxy_with_settings};
use fixtures::pinned::StaticCatalog;

/// A runtime running model 2, one of the two `qwen`s.
#[derive(Debug)]
struct RunningTwin;

#[async_trait]
impl ModelRuntimePort for RunningTwin {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::Internal("not exercised".into()))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        Some(RunningTarget::local(0, 2, "qwen".into(), 16_384, false))
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        None
    }
}

async fn proxy() -> (String, tokio_util::sync::CancellationToken) {
    let catalog = StaticCatalog::numbered(&[(1, "qwen"), (2, "qwen"), (3, "org/coder")]);
    spawn_proxy_with_settings(
        Arc::new(RunningTwin),
        Arc::new(catalog),
        Arc::new(settings_listing("coding")),
    )
    .await
}

/// `GET {base}{path}`: the status and the JSON body.
async fn get(base: &str, path: &str) -> (StatusCode, Value) {
    let response = Client::new()
        .get(format!("{base}{path}"))
        .send()
        .await
        .expect("the proxy answers");
    let status = response.status();
    let body = response
        .json()
        .await
        .expect("a handler's JSON, not axum's empty 404");
    (status, body)
}

/// One path segment for `id`: `/` is the only character an id holds that a
/// segment cannot.
fn segment(id: &str) -> String {
    id.replace('%', "%25").replace('/', "%2F")
}

#[tokio::test]
async fn every_listed_entry_is_answered_as_listed() {
    let (base, cancel) = proxy().await;
    let (_, list) = get(&base, "/v1/models").await;
    let entries = list["data"].as_array().expect("a list").clone();
    let mut answers = Vec::new();
    for entry in &entries {
        let id = entry["id"].as_str().expect("an id");
        answers.push(get(&base, &format!("/v1/models/{}", segment(id))).await);
    }
    cancel.cancel();

    // The fixture holds what it is meant to: a twin's `@` id, a `/` in an
    // id, a profile entry, and the running model's live window.
    let ids: Vec<&str> = entries.iter().filter_map(|e| e["id"].as_str()).collect();
    assert!(ids.iter().any(|id| id.contains('@')), "{ids:?}");
    assert!(ids.contains(&"org/coder"), "{ids:?}");
    assert!(ids.contains(&"org/coder:coding"), "{ids:?}");
    let live = entries
        .iter()
        .find(|e| e["gglib_id"] == 2 && e.get("profile").is_none())
        .expect("model 2 is listed");
    assert_eq!(live["context_window"], 16_384 * 92 / 100, "{live}");

    for (entry, (status, body)) in entries.iter().zip(&answers) {
        assert_eq!(*status, StatusCode::OK, "{entry} -> {body}");
        assert_eq!(body, entry);
    }
}

/// An id the list does not hold is `OpenAI`'s 404, a catalogue id that a chat
/// request would resolve among them, and a suffix that names no profile.
#[tokio::test]
async fn an_id_the_list_does_not_hold_is_404_model_not_found() {
    let (base, cancel) = proxy().await;
    let mut refused = Vec::new();
    for id in ["nope", "3", "qwen:nope"] {
        refused.push((id, get(&base, &format!("/v1/models/{id}")).await));
    }
    cancel.cancel();

    for (id, (status, body)) in refused {
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}: {body}");
        assert_eq!(body["error"]["code"], "model_not_found", "{id}: {body}");
        assert_eq!(
            body["error"]["type"], "invalid_request_error",
            "{id}: {body}"
        );
        assert!(body["error"]["message"].is_string(), "{id}: {body}");
    }
}

/// Behind the same gate as `/v1/models`: a request that claims the tunnel
/// and names no device reaches nothing.
#[tokio::test]
async fn a_tunnelled_request_with_no_device_is_refused() {
    let (base, cancel) = proxy().await;
    let response = Client::new()
        .get(format!("{base}/v1/models/org%2Fcoder"))
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .send()
        .await
        .expect("the proxy answers");
    let status = response.status();
    let body: Value = response.json().await.expect("json");
    cancel.cancel();

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "device_not_paired");
}
