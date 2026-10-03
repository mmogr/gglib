//! `/v1/models` finds the running model and the pinned one by catalog id.
//!
//! A name can belong to more than one model, so the running model's live
//! context window and the pinned filter are both matched by id: two models
//! named `qwen` stay two entries, and only the one that is running, or
//! pinned, is treated as such.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, PinnedSpec, RunningTarget,
};

use fixtures::common::spawn_proxy_with_catalog;
use fixtures::pinned::{StaticCatalog, pin};

/// The context the running model was launched with.
const LIVE_CTX: u64 = 16_384;

/// What `/v1/models` advertises for it, less the 8% safety margin.
const ADVERTISED_LIVE: Option<u64> = Some(LIVE_CTX * 92 / 100);

/// Runtime running model `id`, named `qwen`, and pinned to it when `pinned`.
#[derive(Debug)]
struct Running {
    id: u32,
    pinned: bool,
}

#[async_trait]
impl ModelRuntimePort for Running {
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
        Some(RunningTarget::local(
            0,
            self.id,
            "qwen".into(),
            LIVE_CTX,
            false,
        ))
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        self.pinned.then(|| pin(self.id, "qwen"))
    }
}

/// The `context_window` of every entry `/v1/models` lists, in order, with the
/// second of two models named `qwen` running.
async fn context_windows(pinned: bool) -> Vec<Option<u64>> {
    let catalog = StaticCatalog::numbered(&[(1, "qwen"), (2, "qwen")]);
    let runtime = Arc::new(Running { id: 2, pinned });
    let (base, cancel) = spawn_proxy_with_catalog(runtime, Arc::new(catalog)).await;
    let body: Value = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("json");
    cancel.cancel();
    body["data"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|m| m["context_window"].as_u64())
        .collect()
}

/// Only the running model advertises its live context; the other `qwen` keeps
/// what a launch would give it.
#[tokio::test]
async fn only_the_running_model_advertises_its_live_context() {
    let windows = context_windows(false).await;
    assert_eq!(windows.len(), 2, "{windows:?}");
    assert_ne!(windows[0], ADVERTISED_LIVE, "{windows:?}");
    assert_eq!(windows[1], ADVERTISED_LIVE, "{windows:?}");
}

/// A pinned endpoint keeps only the pinned model, not every model that shares
/// its name.
#[tokio::test]
async fn a_pinned_endpoint_keeps_only_the_pinned_id() {
    let windows = context_windows(true).await;
    assert_eq!(windows, [ADVERTISED_LIVE]);
}
