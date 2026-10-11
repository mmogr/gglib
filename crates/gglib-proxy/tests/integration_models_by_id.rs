//! `/v1/models` carries each model's catalog id, and finds the running model
//! and the pinned one by it.
//!
//! A name can belong to more than one model, so the running model's live
//! context window and the pinned filter are both matched by id: two models
//! named `qwen` stay two entries, and only the one that is running, or
//! pinned, is treated as such. The list also names this machine, which
//! `/health` never does.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, PinnedSpec, RunningTarget,
};

use fixtures::common::{settings_listing, spawn_proxy_with_catalog, spawn_proxy_with_settings};
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

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
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

/// Every entry carries its catalog id; a profile variant carries its base's
/// and names the profile. The `id` a client sends stays the name.
#[tokio::test]
async fn each_entry_carries_its_catalog_id_and_variants_share_it() {
    let catalog = StaticCatalog::numbered(&[(5, "qwen"), (9, "mistral")]);
    let runtime = Arc::new(Running {
        id: 5,
        pinned: false,
    });
    let (base, cancel) = spawn_proxy_with_settings(
        runtime,
        Arc::new(catalog),
        Arc::new(settings_listing("coding")),
    )
    .await;
    let body: Value = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("json");
    cancel.cancel();

    let entries: Vec<(&str, i64, Option<&str>)> = body["data"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|m| {
            (
                m["id"].as_str().expect("an id"),
                m["gglib_id"].as_i64().expect("a gglib_id"),
                m["profile"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("qwen", 5, None),
            ("mistral", 9, None),
            ("qwen:coding", 5, Some("coding")),
            ("mistral:coding", 9, Some("coding")),
        ]
    );
}

/// The list names this machine, sanitised by `machine_name`; `/health`,
/// which answers without credentials, says only that it is up.
#[tokio::test]
async fn the_list_names_the_machine_and_health_does_not() {
    let runtime = Arc::new(Running {
        id: 1,
        pinned: false,
    });
    let (base, cancel) =
        spawn_proxy_with_catalog(runtime, Arc::new(StaticCatalog::new(&["qwen"]))).await;
    let client = Client::new();
    let list: Value = client
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("json");
    let health: Value = client
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("json");
    cancel.cancel();

    let expected = sysinfo::System::host_name()
        .as_deref()
        .and_then(gglib_core::domain::machine_name);
    assert_eq!(list["machine_name"].as_str(), expected.as_deref(), "{list}");
    assert_eq!(health, serde_json::json!({ "status": "ok" }));
}
