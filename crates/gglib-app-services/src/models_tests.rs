//! Tests for [`super::ModelOps`].
//!
//! A `#[path]` sibling rather than an inline `mod tests`, following the split
//! #860 made across the workspace: the tests outweighed the code they cover
//! four to one, and every line of them counted against the module's size
//! ratchet.

use std::sync::Arc;

use tempfile::tempdir;
use tokio::fs;

use super::*;
use crate::error::GuiError;
use crate::sampling_explain::ProvenanceKindDto;
use crate::test_support::test_core;
use gglib_core::ports::{NoopGgufParser, NoopModelRuntime};

/// An emitter that keeps what it was handed, so a test can assert on what
/// a mutation broadcast rather than only on what it returned.
#[derive(Default)]
struct RecordingEmitter {
    events: std::sync::Mutex<Vec<AppEvent>>,
}

impl RecordingEmitter {
    fn events(&self) -> Vec<AppEvent> {
        self.events.lock().expect("emitter lock").clone()
    }
}

impl AppEventEmitter for RecordingEmitter {
    fn emit(&self, event: AppEvent) {
        self.events.lock().expect("emitter lock").push(event);
    }

    fn clone_box(&self) -> Box<dyn AppEventEmitter> {
        Box::new(Self {
            events: std::sync::Mutex::new(self.events()),
        })
    }
}

fn make_ops(core: Arc<AppCore>) -> ModelOps {
    make_ops_with_emitter(core, Arc::new(gglib_core::ports::NoopEmitter::new()))
}

fn make_ops_with_emitter(core: Arc<AppCore>, emitter: Arc<dyn AppEventEmitter>) -> ModelOps {
    ModelOps::new(ModelDeps {
        core,
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(NoopGgufParser),
        emitter,
    })
}

#[tokio::test]
async fn list_returns_empty_on_fresh_db() {
    let core = test_core().await;
    let ops = make_ops(core);
    let models = ops.list().await.expect("list should succeed");
    assert!(models.is_empty());
}

#[tokio::test]
async fn get_unknown_id_returns_not_found() {
    let core = test_core().await;
    let ops = make_ops(core);
    let result = ops.get(999).await;
    assert!(
        matches!(
            result,
            Err(GuiError::NotFound {
                entity: "model",
                ..
            })
        ),
        "expected NotFound, got {result:?}"
    );
}

#[tokio::test]
async fn add_and_list_model() {
    let core = test_core().await;
    let ops = make_ops(core);

    let dir = tempdir().unwrap();
    let gguf_path = dir.path().join("model.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();
    // Canonicalize to resolve macOS /var → /private/var symlinks so the
    // comparison matches the path the service stores after canonicalization.
    let gguf_path = gguf_path.canonicalize().unwrap();

    let req = AddModelRequest {
        file_path: gguf_path.to_str().unwrap().to_string(),
    };

    let added = ops.add(req).await.expect("add should succeed");
    let canonical = std::fs::canonicalize(&gguf_path).unwrap();
    assert_eq!(added.file_path, canonical.to_str().unwrap());

    let models = ops.list().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, added.id);
}

/// **The link production actually crosses.** The duplicate leaves the core
/// as `CoreError::Repository(AlreadyExists)`, and `add` intercepts it here
/// before the blanket `CoreError -> HttpError` conversion ever sees it. A
/// test that exercises only that blanket conversion leaves this arm
/// unpinned: reverting it to `GuiError::Internal` turns the duplicate back
/// into a 500 with every error-mapping test still green.
#[tokio::test]
async fn adding_a_file_already_in_the_library_is_a_conflict() {
    let core = test_core().await;
    let ops = make_ops(core);

    let dir = tempdir().unwrap();
    let gguf_path = dir.path().join("model.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();
    let file_path = gguf_path.to_str().unwrap().to_string();

    ops.add(AddModelRequest {
        file_path: file_path.clone(),
    })
    .await
    .expect("first add should succeed");

    let result = ops.add(AddModelRequest { file_path }).await;
    assert!(
        matches!(result, Err(GuiError::Conflict(_))),
        "expected Conflict, got {result:?}"
    );
}

/// **The duplicate `--force` used to create.** A downloaded model is keyed
/// `hf:<repo>@<sha>#<file>`, but re-importing its file computes a
/// `local:<hash>` key. Nothing conflicted, `file_path` carries no unique
/// index, and the refresh appended a *second* row for one file — the exact
/// failure this change exists to prevent, reintroduced by the flag added
/// to serve it.
///
/// This has to run against the real repository. `MockRepo` upserts on
/// `file_path` while `SqliteModelRepository` upserts on `model_key`, so the
/// core-level double reports "one row, id kept" for input the product
/// duplicates — it cannot observe this bug at all.
#[tokio::test]
async fn forcing_a_downloaded_model_refreshes_it_rather_than_duplicating_it() {
    use gglib_core::domain::NewModel;
    use gglib_core::ports::NoopGgufParser;
    use gglib_core::services::ImportMode;

    let core = test_core().await;

    let dir = tempdir().unwrap();
    let gguf_path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();

    // Registered the way a completed download registers it: with HF
    // provenance, and therefore an `hf:` model key.
    let mut downloaded = NewModel::new(
        "Qwen3-8B".to_string(),
        gguf_path.clone(),
        8.0,
        chrono::Utc::now(),
    );
    downloaded.hf_repo_id = Some("Qwen/Qwen3-8B-GGUF".to_string());
    downloaded.hf_commit_sha = Some("abc123".to_string());
    downloaded.hf_filename = Some("Qwen3-8B-Q4_K_M.gguf".to_string());
    let original = core
        .models()
        .add(downloaded)
        .await
        .expect("registering a download should succeed");

    let refreshed = core
        .models()
        .import_from_file(&gguf_path, &NoopGgufParser, None, ImportMode::Refresh)
        .await
        .expect("--force must refresh rather than fail");

    assert_eq!(
        refreshed.id, original.id,
        "the refresh must land on the downloaded row, not create a new one"
    );
    assert_eq!(
        core.models().list().await.unwrap().len(),
        1,
        "one file is one model"
    );
}

/// **A refresh must not repoint a sharded model at the wrong file.**
///
/// `find_by_path` matches a sharded model through its sibling paths, so
/// `--force` on shard 2 finds the row keyed to shard 1. Landing on it and
/// assigning `file_path = excluded.file_path` would repoint the model at
/// the shard-2 file — which llama.cpp cannot open a split GGUF from, so
/// the model would stop launching. Appending a stray row (the behaviour
/// before the refresh landed on the right row) was survivable; destroying
/// the good row is not.
#[tokio::test]
async fn forcing_from_a_sibling_shard_refuses_rather_than_repointing() {
    use gglib_core::domain::NewModel;
    use gglib_core::ports::NoopGgufParser;
    use gglib_core::services::ImportMode;

    let core = test_core().await;

    let dir = tempdir().unwrap();
    let first = dir.path().join("Qwen3-30B-00001-of-00002.gguf");
    let second = dir.path().join("Qwen3-30B-00002-of-00002.gguf");
    fs::write(&first, b"placeholder").await.unwrap();
    fs::write(&second, b"placeholder").await.unwrap();

    // Seeded with HuggingFace provenance, because that is the only way the
    // product ever produces a sharded row — `file_paths` is set by the
    // download path alone. A local sharded model would give the mutation a
    // `local:` key to collide with and demonstrate a duplicate rather than
    // the repoint this guard exists to prevent.
    let mut sharded = NewModel::new(
        "Qwen3-30B".to_string(),
        first.clone(),
        30.0,
        chrono::Utc::now(),
    );
    sharded.file_paths = Some(vec![first.clone(), second.clone()]);
    sharded.hf_repo_id = Some("Qwen/Qwen3-30B-GGUF".to_string());
    sharded.hf_commit_sha = Some("abc123".to_string());
    sharded.hf_filename = Some("Qwen3-30B-00001-of-00002.gguf".to_string());
    let original = core.models().add(sharded).await.expect("register");

    let err = core
        .models()
        .import_from_file(&second, &NoopGgufParser, None, ImportMode::Refresh)
        .await
        .expect_err("refreshing from shard 2 must be refused");
    assert!(
        matches!(err, gglib_core::ports::CoreError::Validation(_)),
        "got {err:?}"
    );

    let still = core
        .models()
        .get_by_id(original.id)
        .await
        .unwrap()
        .expect("the row must still be there");
    assert_eq!(
        still.file_path,
        std::fs::canonicalize(&first).unwrap(),
        "the model must still point at its first shard"
    );
    assert_eq!(core.models().list().await.unwrap().len(), 1);

    // The other half of the guard: refreshing the model by its *own*
    // first shard is the documented workflow and must be accepted —
    // including when the caller spells that path indirectly. A guard that
    // compared the stored column without resolving it would refuse this
    // and print the same path on both sides of the message.
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let respelled = dir
        .path()
        .join("sub")
        .join("..")
        .join(first.file_name().expect("the shard has a file name"));
    let refreshed = core
        .models()
        .import_from_file(&respelled, &NoopGgufParser, None, ImportMode::Refresh)
        .await
        .expect("refreshing by the model's own first shard must be accepted");
    assert_eq!(refreshed.id, original.id);
    assert_eq!(core.models().list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn add_nonexistent_file_returns_validation_error() {
    let core = test_core().await;
    let ops = make_ops(core);

    let req = AddModelRequest {
        file_path: "/no/such/file.gguf".to_string(),
    };
    let result = ops.add(req).await;
    assert!(
        matches!(result, Err(GuiError::ValidationFailed(_))),
        "expected ValidationFailed, got {result:?}"
    );
}

#[tokio::test]
async fn remove_unknown_id_returns_not_found() {
    let core = test_core().await;
    let ops = make_ops(core);
    let result = ops.remove(999, RemoveModelRequest::default()).await;
    assert!(
        matches!(
            result,
            Err(GuiError::NotFound {
                entity: "model",
                ..
            })
        ),
        "expected NotFound, got {result:?}"
    );
}

/// A runtime that reports one fixed model as running, and records
/// whether `stop_current` was called.
///
/// Stands in for the shared `ProcessManager`-backed runtime `ServerOps`
/// starts models through. Before this fix, `ModelOps` consulted its own
/// `ProcessRunner` instead — a registry `ServerOps` never wrote to — so a
/// model actually running under the proxy looked idle here and the force
/// guard below never fired.
#[derive(Debug)]
struct RunningRuntime {
    model_id: i64,
    port: u16,
    stopped: std::sync::atomic::AtomicBool,
}

impl RunningRuntime {
    fn new(model_id: i64, port: u16) -> Self {
        Self {
            model_id,
            port,
            stopped: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl gglib_core::ports::ModelRuntimePort for RunningRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
        _overrides: gglib_core::ports::LaunchOverrides,
    ) -> Result<gglib_core::ports::Admission, gglib_core::ports::ModelRuntimeError> {
        unimplemented!("not exercised by the remove() tests")
    }

    async fn current_model(&self) -> Option<gglib_core::ports::RunningTarget> {
        None
    }

    async fn list_running(&self) -> Vec<gglib_core::ports::ProcessHandle> {
        vec![gglib_core::ports::ProcessHandle::new(
            self.model_id,
            "running-model".to_string(),
            None,
            self.port,
            0,
        )]
    }

    async fn stop_current(&self) -> Result<(), gglib_core::ports::ModelRuntimeError> {
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

/// Add a placeholder model on disk and register it, returning the DTO.
async fn add_placeholder_model(core: Arc<AppCore>, dir: &tempfile::TempDir) -> GuiModel {
    add_placeholder_model_with(&make_ops(core), dir).await
}

/// The same, through caller-supplied ops — for tests that need to observe
/// what the add itself did, not just what it returned.
async fn add_placeholder_model_with(ops: &ModelOps, dir: &tempfile::TempDir) -> GuiModel {
    let gguf_path = dir.path().join("model.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();
    let gguf_path = gguf_path.canonicalize().unwrap();

    ops.add(AddModelRequest {
        file_path: gguf_path.to_str().unwrap().to_string(),
    })
    .await
    .expect("add should succeed")
}

/// Library membership changes must reach every client, not just the one
/// that made them.
///
/// The GUI refetches its own list after its own edit, which makes a single
/// tab look correct and hides the real gap: a model added from the CLI, a
/// second window, or the daemon is invisible until someone hits refresh.
/// `AppEvent` has carried these three variants — and `event_name()` has
/// mapped them — since before anything emitted one.
#[tokio::test]
async fn adding_a_model_broadcasts_it() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let emitter = Arc::new(RecordingEmitter::default());
    let ops = make_ops_with_emitter(core, Arc::clone(&emitter) as Arc<dyn AppEventEmitter>);

    let added = add_placeholder_model_with(&ops, &dir).await;

    match emitter.events().as_slice() {
        [AppEvent::ModelAdded { model }] => {
            assert_eq!(model.id, added.id);
            assert_eq!(model.name, added.name);
            assert_eq!(model.file_path, added.file_path);
        }
        other => panic!("expected exactly one ModelAdded, got {other:?}"),
    }
}

#[tokio::test]
async fn updating_a_model_broadcasts_the_row_as_stored() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let emitter = Arc::new(RecordingEmitter::default());
    let ops = make_ops_with_emitter(core, Arc::clone(&emitter) as Arc<dyn AppEventEmitter>);
    let added = add_placeholder_model_with(&ops, &dir).await;

    let updated = ops
        .update(
            added.id,
            UpdateModelRequest {
                name: Some("renamed".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("update should succeed");

    match emitter.events().as_slice() {
        [
            AppEvent::ModelAdded { .. },
            AppEvent::ModelUpdated { model },
        ] => {
            assert_eq!(model.id, added.id);
            assert_eq!(
                model.name, updated.name,
                "the event must carry the stored row, not the request"
            );
        }
        other => panic!("expected ModelAdded then ModelUpdated, got {other:?}"),
    }
}

#[tokio::test]
async fn removing_a_model_broadcasts_its_id() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let emitter = Arc::new(RecordingEmitter::default());
    let ops = make_ops_with_emitter(core, Arc::clone(&emitter) as Arc<dyn AppEventEmitter>);
    let added = add_placeholder_model_with(&ops, &dir).await;

    ops.remove(added.id, RemoveModelRequest::default())
        .await
        .expect("remove should succeed");

    match emitter.events().as_slice() {
        [
            AppEvent::ModelAdded { .. },
            AppEvent::ModelRemoved { model_id },
        ] => {
            assert_eq!(*model_id, added.id);
        }
        other => panic!("expected ModelAdded then ModelRemoved, got {other:?}"),
    }
}

/// A refused mutation must stay silent — a broadcast here would tell every
/// client to drop a model that is still in the library.
#[tokio::test]
async fn a_blocked_remove_broadcasts_nothing() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let added = add_placeholder_model(Arc::clone(&core), &dir).await;

    let emitter = Arc::new(RecordingEmitter::default());
    let ops = ModelOps::new(ModelDeps {
        core,
        runtime: Arc::new(RunningRuntime::new(added.id, 5500)) as Arc<dyn ModelRuntimePort>,
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::clone(&emitter) as Arc<dyn AppEventEmitter>,
    });

    let result = ops.remove(added.id, RemoveModelRequest::default()).await;

    assert!(matches!(result, Err(GuiError::Conflict(_))));
    assert!(
        emitter.events().is_empty(),
        "a refused remove must not announce a removal: {:?}",
        emitter.events()
    );
}

/// Regression test for the `ModelOps`/`ServerOps` registry split: `remove`
/// must consult the same runtime models are actually started through, so
/// a model the proxy reports running blocks deletion here too.
#[tokio::test]
async fn remove_blocks_when_the_shared_runtime_reports_the_model_running() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let added = add_placeholder_model(Arc::clone(&core), &dir).await;

    let runtime = Arc::new(RunningRuntime::new(added.id, 5500));
    let ops = ModelOps::new(ModelDeps {
        core,
        runtime: Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(gglib_core::ports::NoopEmitter::new()),
    });

    let result = ops.remove(added.id, RemoveModelRequest::default()).await;
    assert!(
        matches!(result, Err(GuiError::Conflict(_))),
        "expected Conflict while the shared runtime reports the model running, got {result:?}"
    );
    assert!(
        !runtime.stopped(),
        "a blocked (non-forced) remove must not stop the server"
    );
}

/// `force=true` must stop the server through the same shared runtime
/// `ServerOps` uses, not a disconnected registry that never saw it start.
#[tokio::test]
async fn remove_with_force_stops_the_server_through_the_shared_runtime() {
    let core = test_core().await;
    let dir = tempdir().unwrap();
    let added = add_placeholder_model(Arc::clone(&core), &dir).await;

    let runtime = Arc::new(RunningRuntime::new(added.id, 5500));
    let ops = ModelOps::new(ModelDeps {
        core,
        runtime: Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(gglib_core::ports::NoopEmitter::new()),
    });

    let result = ops
        .remove(added.id, RemoveModelRequest { force: true })
        .await;
    assert!(result.is_ok(), "force=true should proceed: {result:?}");
    assert!(
        runtime.stopped(),
        "force=true must stop the server via the shared runtime"
    );
}

#[tokio::test]
async fn list_tags_empty_on_fresh_db() {
    let core = test_core().await;
    let ops = make_ops(core);
    let tags = ops.list_tags().await.expect("list_tags should succeed");
    assert!(tags.is_empty());
}

/// End-to-end: drive `ModelOps::update` with `UpdateModelRequest` values
/// built from real `serde_json::from_str` payloads (not constructed
/// directly in Rust) to prove the double-`Option` null-clearing fix
/// works across the actual JSON boundary, not just in isolated
/// deserialization tests.
#[tokio::test]
async fn update_server_defaults_json_round_trip() {
    let core = test_core().await;
    let ops = make_ops(core);

    let dir = tempdir().unwrap();
    let gguf_path = dir.path().join("model.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();
    let gguf_path = gguf_path.canonicalize().unwrap();

    let added = ops
        .add(AddModelRequest {
            file_path: gguf_path.to_str().unwrap().to_string(),
        })
        .await
        .expect("add should succeed");
    assert!(
        added.server_defaults.is_none(),
        "fresh model has no override"
    );

    // 1. Set server_defaults via a populated-object JSON payload.
    let set_req: UpdateModelRequest =
        serde_json::from_str(r#"{"serverDefaults": {"contextLength": 8192}}"#).unwrap();
    let updated = ops
        .update(added.id, set_req)
        .await
        .expect("update should succeed");
    assert_eq!(
        updated
            .server_defaults
            .as_ref()
            .and_then(|c| c.context_length),
        Some(8192),
        "server_defaults.contextLength should be set from JSON"
    );

    // 2. Omitted key is a no-op — other fields change, override survives.
    let noop_req: UpdateModelRequest = serde_json::from_str(r#"{"name": "Renamed"}"#).unwrap();
    let after_noop = ops
        .update(added.id, noop_req)
        .await
        .expect("update should succeed");
    assert_eq!(after_noop.name, "Renamed");
    assert_eq!(
        after_noop
            .server_defaults
            .as_ref()
            .and_then(|c| c.context_length),
        Some(8192),
        "omitted serverDefaults key must not clear the existing override"
    );

    // 3. Explicit JSON null clears the override.
    let clear_req: UpdateModelRequest =
        serde_json::from_str(r#"{"serverDefaults": null}"#).unwrap();
    let cleared = ops
        .update(added.id, clear_req)
        .await
        .expect("update should succeed");
    assert!(
        cleared.server_defaults.is_none(),
        "explicit JSON null must clear server_defaults"
    );
}

// ── explain_sampling ──────────────────────────────────────────────────
//
// The resolution itself is covered in `sampling_explain`; these cover the
// wiring — model lookup, settings load, and profile selection.

/// Import a placeholder model and return its id.
async fn seed_model(ops: &ModelOps, dir: &tempfile::TempDir) -> i64 {
    let gguf_path = dir.path().join("model.gguf");
    fs::write(&gguf_path, b"placeholder").await.unwrap();
    ops.add(AddModelRequest {
        file_path: gguf_path.to_str().unwrap().to_string(),
    })
    .await
    .expect("add should succeed")
    .id
}

fn profile(name: &str, temperature: f32) -> gglib_core::domain::InferenceProfile {
    gglib_core::domain::InferenceProfile {
        name: name.to_owned(),
        description: None,
        config: gglib_core::domain::InferenceConfig {
            temperature: Some(temperature),
            ..Default::default()
        },
        list_in_models: false,
    }
}

#[tokio::test]
async fn explain_sampling_falls_back_to_the_floor_when_nothing_is_stored() {
    let core = test_core().await;
    let ops = make_ops(Arc::clone(&core));
    let dir = tempdir().unwrap();
    let id = seed_model(&ops, &dir).await;

    let dto = ops.explain_sampling(id, None).await.expect("explain");

    assert_eq!(dto.resolved.temperature, Some(0.7));
    assert!(dto.profile.is_none());
    assert!(!dto.is_reasoning);
    assert!(
        dto.sources
            .iter()
            .all(|entry| entry.layer.is_none() && entry.kind != ProvenanceKindDto::Layer),
        "no layer should have supplied anything: {:?}",
        dto.sources
    );
}

#[tokio::test]
async fn explain_sampling_unknown_id_returns_not_found() {
    let core = test_core().await;
    let ops = make_ops(core);
    let result = ops.explain_sampling(999, None).await;
    assert!(
        matches!(
            result,
            Err(GuiError::NotFound {
                entity: "model",
                ..
            })
        ),
        "expected NotFound, got {result:?}"
    );
}

/// A named profile that does not exist is a caller error, not a reason to
/// answer a different question.
#[tokio::test]
async fn explain_sampling_unknown_profile_names_the_configured_ones() {
    let core = test_core().await;
    let ops = make_ops(Arc::clone(&core));
    let dir = tempdir().unwrap();
    let id = seed_model(&ops, &dir).await;

    core.settings()
        .update(gglib_core::SettingsUpdate {
            inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
            ..Default::default()
        })
        .await
        .unwrap();

    let result = ops.explain_sampling(id, Some("codign")).await;
    let Err(GuiError::ValidationFailed(message)) = result else {
        panic!("expected ValidationFailed, got {result:?}");
    };
    assert!(message.contains("codign"), "{message}");
    assert!(message.contains("coding"), "{message}");
}

#[tokio::test]
async fn explain_sampling_applies_a_named_profile_over_global_settings() {
    let core = test_core().await;
    let ops = make_ops(Arc::clone(&core));
    let dir = tempdir().unwrap();
    let id = seed_model(&ops, &dir).await;

    core.settings()
        .update(gglib_core::SettingsUpdate {
            inference_defaults: Some(Some(gglib_core::domain::InferenceConfig {
                temperature: Some(0.4),
                ..Default::default()
            })),
            inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
            ..Default::default()
        })
        .await
        .unwrap();

    let unprofiled = ops.explain_sampling(id, None).await.expect("explain");
    assert_eq!(unprofiled.resolved.temperature, Some(0.4));

    let profiled = ops
        .explain_sampling(id, Some("coding"))
        .await
        .expect("explain");
    assert_eq!(profiled.profile.as_deref(), Some("coding"));
    assert_eq!(profiled.resolved.temperature, Some(0.2));
}

#[tokio::test]
async fn explain_sampling_reads_the_reasoning_tag_from_the_stored_model() {
    let core = test_core().await;
    let ops = make_ops(Arc::clone(&core));
    let dir = tempdir().unwrap();
    let id = seed_model(&ops, &dir).await;

    ops.add_tag(id, "reasoning".to_owned()).await.unwrap();

    let dto = ops.explain_sampling(id, None).await.expect("explain");
    assert!(dto.is_reasoning);
    assert_eq!(dto.resolved.presence_penalty, Some(1.0));
}
