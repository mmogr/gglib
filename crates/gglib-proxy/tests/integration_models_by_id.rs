//! `/v1/models` carries each model's catalog id, and finds the running model
//! and the pinned one by it.
//!
//! A name can belong to more than one model, so the running model's live
//! context window and the pinned filter are both matched by id: two models
//! named `qwen` stay two entries, and only the one that is running, or
//! pinned, is treated as such. Each is listed under an id of its own,
//! `qwen@Q6_K` or `qwen@2`, and a chat that sends one back is admitted as
//! that model. The list also names this machine, when the serve's
//! `HostFacts` can, which `/health` never does.

mod fixtures;

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, PinnedSpec, RunningTarget,
};
use gglib_core::services::CatalogPortImpl;
use gglib_core::{Model, ModelRepository, NewModel, RepositoryError};
use gglib_proxy::{HostFacts, ServeConfig};

use fixtures::common::{settings_listing, spawn_proxy_with_catalog, spawn_proxy_with_settings};
use fixtures::pinned::{StaticCatalog, pin};
use fixtures::spawn::{Spawned, defaults, spawn};

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

/// Every entry `/v1/models` lists, in order, with the second of two models
/// named `qwen` running.
async fn twin_entries(pinned: bool) -> Vec<Value> {
    let catalog = StaticCatalog::numbered(&[(1, "qwen"), (2, "qwen")]);
    let runtime = Arc::new(Running { id: 2, pinned });
    let (base, cancel) = spawn_proxy_with_catalog(runtime, Arc::new(catalog)).await;
    let body = get_json(&base, "/v1/models").await;
    cancel.cancel();
    body["data"].as_array().expect("a list").clone()
}

/// The `context_window` of every entry [`twin_entries`] lists.
async fn context_windows(pinned: bool) -> Vec<Option<u64>> {
    twin_entries(pinned)
        .await
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
/// and names the profile. A name no other model has is the `id` a client
/// sends.
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

/// The pinned twin is listed under its own id, worked out over the whole
/// catalogue: alone in the list, it would be listed as `qwen`, which names
/// the other twin, the lowest row of that name.
#[tokio::test]
async fn a_pinned_twin_is_listed_under_the_id_that_finds_it() {
    let ids: Vec<String> = twin_entries(true)
        .await
        .iter()
        .map(|m| m["id"].as_str().expect("an id").to_owned())
        .collect();
    assert_eq!(ids, ["qwen@2"]);
}

/// A store over fixed rows that implements only the required lookups, so
/// the catalogue port over it resolves identifiers by the shared policy
/// (`ModelRepository::get_by_identifier`), `@` forms included.
struct Library(Vec<Model>);

#[async_trait]
impl ModelRepository for Library {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(self.0.clone())
    }

    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        self.0
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(format!("id={id}")))
    }

    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        self.0
            .iter()
            .filter(|m| m.name == name)
            .min_by_key(|m| m.id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(format!("name={name}")))
    }

    async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
        Ok(None)
    }

    async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
        unimplemented!("not exercised")
    }

    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        unimplemented!("not exercised")
    }

    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        unimplemented!("not exercised")
    }
}

/// A runtime that records which model each admission asks for, then refuses
/// it: which row a chat resolved to is all these tests read.
#[derive(Debug, Default)]
struct Admits(Mutex<Vec<String>>);

#[async_trait]
impl ModelRuntimePort for Admits {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.0.lock().unwrap().push(model_name.to_owned());
        Err(ModelRuntimeError::Internal("recorded".into()))
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

/// Model `id`, named `name`, in `quantization`.
fn quantized(id: u32, name: &str, quantization: &str) -> Model {
    Model {
        quantization: Some(quantization.to_owned()),
        ..StaticCatalog::row(id, name)
    }
}

/// Two quantizations of `qwen` are listed under two ids, a profile variant
/// of each builds on its own, and a chat that sends any of them back is
/// admitted as that row, never its twin. `mistral`, whose name nobody shares,
/// keeps it.
#[tokio::test]
async fn two_quantizations_of_one_name_are_two_ids_and_two_models() {
    let library = Library(vec![
        quantized(3, "qwen", "Q4_K_M"),
        quantized(8, "qwen", "Q6_K"),
        quantized(5, "mistral", "Q4_K_M"),
    ]);
    let runtime = Arc::new(Admits::default());
    let (base, cancel) = spawn_proxy_with_settings(
        Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
        Arc::new(CatalogPortImpl::new(Arc::new(library))),
        Arc::new(settings_listing("coding")),
    )
    .await;

    let list = get_json(&base, "/v1/models").await;
    let ids: Vec<&str> = list["data"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|m| m["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(
        ids,
        [
            "qwen@Q4_K_M",
            "qwen@Q6_K",
            "mistral",
            "qwen@Q4_K_M:coding",
            "qwen@Q6_K:coding",
            "mistral:coding",
        ]
    );

    let client = Client::new();
    for model in [
        "qwen@Q6_K",
        "qwen@Q4_K_M",
        "qwen@8",
        "qwen@Q6_K:coding",
        "qwen",
    ] {
        client
            .post(format!("{base}/v1/chat/completions"))
            .json(&serde_json::json!({
                "model": model,
                "messages": [{ "role": "user", "content": "hi" }],
            }))
            .send()
            .await
            .expect("the proxy answers");
    }
    cancel.cancel();

    // A bare shared name is the lowest row, as before the twins were listed
    // apart.
    assert_eq!(*runtime.0.lock().unwrap(), ["8", "3", "8", "8", "3"]);
}

/// A proxy over one model, `qwen`, told what `host` can read.
async fn spawn_with_host(host: HostFacts) -> Spawned {
    spawn(ServeConfig {
        runtime_port: Arc::new(Running {
            id: 1,
            pinned: false,
        }),
        catalog_port: Arc::new(StaticCatalog::new(&["qwen"])),
        host,
        ..defaults().await
    })
    .await
}

/// `GET {base}{path}`, as JSON.
async fn get_json(base: &str, path: &str) -> Value {
    Client::new()
        .get(format!("{base}{path}"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("json")
}

/// A host whose name reader answers `name`.
fn named(name: &'static str) -> HostFacts {
    HostFacts {
        machine_name: Some(Arc::new(move || Some(name.to_owned()))),
        ..HostFacts::none()
    }
}

/// The list names this machine, cut by `machine_name` to its first label;
/// `/health`, which answers without credentials, says only that it is up.
#[tokio::test]
async fn the_list_names_the_machine_and_health_does_not() {
    let proxy = spawn_with_host(named("Desk.local")).await;
    let list = get_json(&proxy.base, "/v1/models").await;
    let health = get_json(&proxy.base, "/health").await;
    proxy.cancel.cancel();

    assert_eq!(list["machine_name"], "Desk", "{list}");
    assert_eq!(health, serde_json::json!({ "status": "ok" }));
}

/// A host that cannot name itself leaves the field out of the list, which
/// still lists its models.
#[tokio::test]
async fn a_host_with_no_name_leaves_it_out_of_the_list() {
    let proxy = spawn_with_host(HostFacts::none()).await;
    let list = get_json(&proxy.base, "/v1/models").await;
    proxy.cancel.cancel();

    assert!(list.get("machine_name").is_none(), "{list}");
    assert_eq!(list["data"][0]["id"], "qwen", "{list}");
}

/// The name is read on every listing, so a rename shows on the next one.
#[tokio::test]
async fn the_name_is_read_on_every_listing() {
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&reads);
    let proxy = spawn_with_host(HostFacts {
        machine_name: Some(Arc::new(move || {
            Some(format!("desk-{}", counted.fetch_add(1, Ordering::SeqCst)))
        })),
        ..HostFacts::none()
    })
    .await;
    let first = get_json(&proxy.base, "/v1/models").await;
    let second = get_json(&proxy.base, "/v1/models").await;
    proxy.cancel.cancel();

    assert_eq!(first["machine_name"], "desk-0", "{first}");
    assert_eq!(second["machine_name"], "desk-1", "{second}");
}
