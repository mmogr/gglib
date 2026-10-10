//! `/v1/models` while a model is running: a llama-server's model is listed
//! with the context it was launched at, and an image model on `sd-server`
//! with none, even running, even though a context is recorded on its target;
//! and loading one answers context 0 and narrates its launch.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::{LaunchDecision, LaunchNarration, RuntimeKind};
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ModelRuntimePort,
    RunningTarget,
};
use reqwest::Client;
use serde_json::Value;

use fixtures::common::spawn_proxy_with_catalog;
use fixtures::images::{DRAWS, SEES, Sight};

/// A runtime whose primary slot holds `target`; it admits nothing.
#[derive(Debug)]
struct Running(RunningTarget);

#[async_trait]
impl ModelRuntimePort for Running {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::ModelNotFound(model_name.to_owned()))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        Some(self.0.clone())
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// The `/v1/models` entry for `name` while `target` runs.
async fn listed_while(target: RunningTarget, name: &str) -> Value {
    let (base, cancel) = spawn_proxy_with_catalog(
        Arc::new(Running(target)) as Arc<dyn ModelRuntimePort>,
        Arc::new(Sight) as Arc<dyn ModelCatalogPort>,
    )
    .await;
    let listed: Value = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    cancel.cancel();
    listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == name)
        .unwrap_or_else(|| panic!("{name} is listed: {listed}"))
        .clone()
}

#[tokio::test]
async fn a_running_image_model_has_no_context_window() {
    let target = RunningTarget::local(9_999, 3, DRAWS.to_owned(), 4096, false)
        .with_runtime(RuntimeKind::StableDiffusion);

    let entry = listed_while(target, DRAWS).await;

    assert!(entry.get("context_window").is_none(), "{entry}");
}

/// The control: the same path gives a running chat model its launched
/// context, so the test above is not passing for want of a running model.
#[tokio::test]
async fn a_running_chat_model_is_listed_with_its_launched_context() {
    let target = RunningTarget::local(9_999, 1, SEES.to_owned(), 4096, false);

    let entry = listed_while(target, SEES).await;

    assert!(entry["context_window"].as_u64().is_some(), "{entry}");
}

/// A runtime that loads the image model `DRAWS` through `sd-server`.
#[derive(Debug)]
struct Draws;

#[async_trait]
impl ModelRuntimePort for Draws {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let mut narration = LaunchNarration::new(model_name, None, 0);
        narration.push(LaunchDecision::new(
            "runtime",
            "stable-diffusion.cpp master-948-228c707",
            "installed",
        ));
        Ok(Admission::detached(
            RunningTarget::local(9_998, 3, model_name.to_owned(), 4096, true)
                .with_runtime(RuntimeKind::StableDiffusion)
                .with_narration(narration),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// `gglib serve <image model>`'s load, over HTTP: answered with context 0,
/// and the launch's narration is on `/v1/proxy/status`.
#[tokio::test]
async fn loading_an_image_model_answers_no_context_and_narrates_the_launch() {
    let (base, cancel) = spawn_proxy_with_catalog(
        Arc::new(Draws) as Arc<dyn ModelRuntimePort>,
        Arc::new(Sight) as Arc<dyn ModelCatalogPort>,
    )
    .await;
    let client = Client::new();

    let loaded: Value = client
        .post(format!("{base}/v1/models/{DRAWS}/load"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let status: Value = client
        .get(format!("{base}/v1/proxy/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    cancel.cancel();

    assert_eq!(loaded["model"], DRAWS, "{loaded}");
    assert_eq!(loaded["started"], true, "{loaded}");
    assert_eq!(loaded["context"], 0, "{loaded}");
    let launch = &status["launch"];
    assert_eq!(launch["model_name"], DRAWS, "{status}");
    assert_eq!(
        launch["decisions"][0]["value"], "stable-diffusion.cpp master-948-228c707",
        "{status}"
    );
}
