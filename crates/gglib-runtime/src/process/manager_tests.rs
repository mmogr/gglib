//! `ProcessManager`: the pin, and what a fresh manager reports.

use super::*;

// ---------------------------------------------------------------
// Pinned mode
// ---------------------------------------------------------------

use crate::process::RuntimeBinaries;
use crate::process::residency::residency_tests::{StubCatalog, pin};

fn manager() -> ProcessManager {
    ProcessManager::new(
        9000,
        RuntimeBinaries::llama_only("llama-server"),
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    )
}

/// Pinned to `qwen2.5`, which the stub catalog holds as id 3.
fn pinned_manager() -> ProcessManager {
    let manager = manager();
    manager.set_pin(Some(pin(3, "qwen2.5")));
    manager
}

async fn admit(manager: &ProcessManager, model: &str) -> ModelRuntimeError {
    manager
        .admit(model, None, Some(4096), LaunchOverrides::default())
        .await
        .expect_err("the stub's models have no files, so nothing is admitted")
}

/// The guard has to sit on the real entry point, not just on `ResidentSet` —
/// this is what a proxy request actually calls.
#[tokio::test]
async fn admit_rejects_a_foreign_model() {
    let err = admit(&pinned_manager(), "llama-3-8b").await;

    assert!(
        matches!(err, ModelRuntimeError::PinnedModelMismatch { .. }),
        "expected PinnedModelMismatch, got {err:?}"
    );
}

/// A pinned endpoint answers to its own model's id, not just its name: both
/// get past the pin and stop only at the launch, whose file the stub lacks.
#[tokio::test]
async fn a_pinned_endpoint_admits_its_own_id() {
    for model in ["3", "qwen2.5"] {
        let err = admit(&pinned_manager(), model).await;
        assert!(
            matches!(err, ModelRuntimeError::ModelFileNotFound(_)),
            "{model} should pass the pin and reach the launch, got {err:?}"
        );
    }
}

/// A foreign model is refused after it resolves — the refusal names the model
/// id 7 resolved to — and before it queues, so it neither waits behind the
/// pinned model nor displaces it.
#[tokio::test]
async fn a_foreign_model_is_refused_after_resolving_before_queueing() {
    let manager = pinned_manager();
    match admit(&manager, "7").await {
        ModelRuntimeError::PinnedModelMismatch {
            expected,
            requested,
        } => assert_eq!(
            (expected.as_str(), requested.as_str()),
            ("qwen2.5", "llama-3-8b")
        ),
        other => panic!("expected PinnedModelMismatch, got {other:?}"),
    }

    let snapshot = manager.admission_snapshot();
    assert_eq!(snapshot.waiting(), 0, "nothing should be left queued");
    assert_eq!(snapshot.total_swaps, 0);
}

/// A model the catalog does not hold is not found, pinned or not — a pinned
/// endpoint does not call it someone else's model.
#[tokio::test]
async fn an_unknown_model_is_not_found_pinned_or_not() {
    for manager in [manager(), pinned_manager()] {
        let err = admit(&manager, "anything").await;
        assert!(
            matches!(err, ModelRuntimeError::ModelNotFound(_)),
            "expected ModelNotFound, got {err:?}"
        );
    }
}

/// Pinning must not leak into the ordinary proxy manager.
#[tokio::test]
async fn an_unpinned_manager_admits_any_model() {
    let err = admit(&manager(), "llama-3-8b").await;

    assert!(
        matches!(err, ModelRuntimeError::ModelFileNotFound(_)),
        "unpinned manager must not reject on identity, got {err:?}"
    );
}

/// An unknown model must fail immediately rather than joining the queue and
/// waiting out a swap only to discover nobody has it.
#[tokio::test]
async fn an_unknown_model_fails_without_queueing() {
    let manager = manager();
    let _ = admit(&manager, "nope").await;

    let snapshot = manager.admission_snapshot();
    assert_eq!(snapshot.waiting(), 0, "nothing should be left queued");
    assert_eq!(snapshot.total_swaps, 0);
}

/// The read side of the guard: callers that want to avoid provoking a
/// mismatch — `/v1/models`, which should not advertise a model that can
/// only be refused — need the pin without attempting a request.
#[test]
fn pinned_manager_reports_its_model() {
    let pinned = pinned_manager().pinned().expect("pinned");
    assert_eq!((pinned.id, pinned.name.as_str()), (3, "qwen2.5"));
}

/// Reporting must agree with admission: a manager that admits any model
/// must not name one, or callers would narrow what they offer for no
/// reason.
#[test]
fn an_unpinned_manager_reports_no_pinned_model() {
    assert!(manager().pinned().is_none());
}

#[tokio::test]
async fn list_running_is_empty_with_no_servers() {
    assert!(manager().list_running().await.is_empty());
}

#[tokio::test]
async fn test_is_loading() {
    assert!(!manager().is_loading());
}

/// A fresh manager holds nothing and has done nothing.
#[test]
fn a_fresh_manager_reports_an_empty_resident_set() {
    let snapshot = manager().admission_snapshot();
    assert!(snapshot.slots.is_empty());
    assert!(snapshot.queued.is_empty());
    assert_eq!(snapshot.total_swaps, 0);
    assert_eq!(snapshot.secondary_slot.state, "available");
}

#[test]
fn a_fresh_manager_has_no_current_model() {
    assert!(manager().current_model().is_none());
}

// ---------------------------------------------------------------
// Holds, through the manager and the port
// ---------------------------------------------------------------

/// A manager whose primary holds model 1 at port 8001, idle.
fn manager_with_resident() -> Arc<ProcessManager> {
    use crate::process::admission::{PRIMARY_SLOT, Resident};
    let manager = Arc::new(manager());
    let resident = Resident {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        model_id: 1,
        model_name: "qwen".to_owned(),
        context_size: 4096,
        port: 8001,
        projector: None,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: gglib_core::domain::CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: tokio::time::Instant::now(),
        weights_bytes: 1024,
    };
    drop(manager.residency.queue().install(PRIMARY_SLOT, resident));
    manager
}

/// Whether the queue would refuse to recycle the primary, putting it back
/// when it would not.
fn primary_is_held(manager: &ProcessManager) -> bool {
    use crate::process::admission::PRIMARY_SLOT;
    let queue = manager.residency.queue();
    let Ok(previous) = queue.evict_unheld(PRIMARY_SLOT) else {
        return true;
    };
    drop(queue.install(PRIMARY_SLOT, previous.expect("a resident")));
    false
}

#[test]
fn the_manager_holds_a_resident_in_its_queue() {
    let manager = manager_with_resident();

    let hold = manager.hold(8001, 1).expect("model 1 is on 8001");
    assert!(primary_is_held(&manager));
    drop(hold);
    assert!(!primary_is_held(&manager));
}

#[test]
fn the_runtime_port_holds_through_the_manager() {
    use gglib_core::ports::ModelRuntimePort as _;
    let manager = manager_with_resident();
    let port = crate::ports_impl::RuntimePortImpl::new(Arc::clone(&manager));

    assert!(port.hold(8001, 2).is_none(), "model 2 is not on 8001");
    let hold = port.hold(8001, 1).expect("model 1 is on 8001");
    assert!(primary_is_held(&manager));
    drop(hold);
    assert!(!primary_is_held(&manager));
}

/// Automatic recovery waits for a run: `recycle_current` refuses while the
/// primary is held and leaves it resident, and stops it once nothing holds it.
#[tokio::test]
async fn the_runtime_port_recycles_the_primary_only_when_no_run_holds_it() {
    use gglib_core::ports::ModelRuntimePort as _;
    let manager = manager_with_resident();
    let port = crate::ports_impl::RuntimePortImpl::new(Arc::clone(&manager));

    let hold = port.hold(8001, 1).expect("model 1 is on 8001");
    let refused = port.recycle_current().await.unwrap_err();
    assert!(matches!(refused, ModelRuntimeError::AdmissionTimeout(_)));
    assert!(manager.current_model().is_some(), "the held model stays");

    drop(hold);
    port.recycle_current().await.expect("recycled");
    assert!(manager.current_model().is_none());
}

/// A person's stop does not wait for a run.
#[tokio::test]
async fn a_stop_takes_the_primary_even_while_a_run_holds_it() {
    use gglib_core::ports::ModelRuntimePort as _;
    let manager = manager_with_resident();
    let port = crate::ports_impl::RuntimePortImpl::new(Arc::clone(&manager));

    let _hold = port.hold(8001, 1).expect("model 1 is on 8001");
    // The fixture spawned no process, so the kill reports none to stop; the
    // slot is emptied before it.
    let _ = port.stop_current().await;
    assert!(manager.current_model().is_none());
}

// ---------------------------------------------------------------
// The runtime, carried from the resident and the process outward
// ---------------------------------------------------------------

/// An `sd-server` resident is reported as one everywhere the manager
/// projects its slots: the routing target and the dashboard snapshot.
#[test]
fn a_stable_diffusion_resident_is_reported_with_its_runtime() {
    use crate::process::admission::{PRIMARY_SLOT, Resident};
    use gglib_core::domain::RuntimeKind;

    let manager = manager_with_resident();
    let llama = manager.current_model().expect("a primary");
    assert_eq!(llama.runtime, RuntimeKind::Llama);

    let previous = manager.residency.queue().evict(PRIMARY_SLOT).expect("qwen");
    let sd = Resident {
        model_name: "flux".to_owned(),
        runtime: RuntimeKind::StableDiffusion,
        ..previous
    };
    drop(manager.residency.queue().install(PRIMARY_SLOT, sd));

    assert_eq!(
        manager.current_model().expect("a primary").runtime,
        RuntimeKind::StableDiffusion
    );
    assert_eq!(
        manager.admission_snapshot().slots[0].runtime,
        RuntimeKind::StableDiffusion
    );
}

/// A running `sd-server` is listed with its runtime.
#[cfg(unix)]
#[tokio::test]
async fn a_running_sd_server_is_listed_with_its_runtime() {
    use crate::sd::SdServerConfig;
    use gglib_core::domain::{ImageFamily, RuntimeKind};
    use std::os::unix::fs::PermissionsExt;

    // Unused by any other test in this binary, for the reason
    // `residency::launch_tests` gives.
    const SD_ID: u32 = 999_022;

    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let script = dir.path().join("sd-server");
    std::fs::write(&script, "#!/bin/sh\nexec sleep 30\n").expect("write script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let model = dir.path().join("sdxl.safetensors");
    std::fs::write(&model, b"not really a checkpoint").expect("write model file");
    let manager = ProcessManager::new(
        19_540,
        RuntimeBinaries {
            llama: "/nonexistent/llama-server".into(),
            sd: script,
        },
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );

    let sd = SdServerConfig {
        model_id: i64::from(SD_ID),
        model_name: "sdxl".to_owned(),
        model_path: model,
        family: ImageFamily::Sdxl,
        components: Vec::new(),
        port: None,
    };
    manager
        .core
        .write()
        .await
        .spawn(crate::process::SpawnConfig::Sd(sd))
        .await
        .expect("spawn");
    let listed: Vec<RuntimeKind> = manager
        .list_running()
        .await
        .into_iter()
        .map(|handle| handle.runtime)
        .collect();
    manager.core.write().await.kill(SD_ID).await.ok();
    crate::pidfile::delete_pidfile(i64::from(SD_ID)).ok();

    assert_eq!(listed, [RuntimeKind::StableDiffusion]);
}
