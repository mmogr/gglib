//! An image model is launched through `sd-server`: refused before the queue
//! when the runtime or a file is missing, with the resident set untouched;
//! placed in the second slot and recorded with its runtime and components
//! when it launches; recycled when a component is relinked, never for a
//! context. And any launch stops the model it displaces before it checks
//! its files again, so a late refusal leaves no server running unknown.
//!
//! Unix-only for the stand-in binaries, shell scripts that record their
//! arguments and sleep; not for the behaviour.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{
    CacheRamHealth, ComponentRole, ImageFamily, ModelComponent, ModelSamplingDefaults, RuntimeKind,
    SecondarySlotDecision,
};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, ModelRuntimeError, ServerConfig};
use gglib_core::server_config::{ContextSizeSource, ServerConfigOptions};
use tokio::sync::RwLock;
use tokio::time::Instant;

use super::launch::LaunchRequest;
use super::launch::launch_files::preflight;
use super::residency_tests::{OneModel, launch_spec};
use super::resident_match::launched_differently;
use super::{ResidentSet, vram};
use crate::process::admission::{AdmissionDecision, Candidate, PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::sd::fake_server::FakeSdServer;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const RESIDENT_ID: u32 = 999_201;
const IMAGE_ID: u32 = 999_202;
const LATE_ID: u32 = 999_203;
const DISPLACED_ID: u32 = 999_204;
const PLACED_ID: u32 = 999_205;
const HELD_ID: u32 = 999_206;

const GIB: u64 = 1024 * 1024 * 1024;

/// A file that exists, under `dir`.
fn file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"x").unwrap();
    path
}

/// A stand-in server at `dir/name` that writes its arguments, one to a line,
/// to `dir/name.args`, then sleeps until it is killed.
fn sleeper(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let binary = dir.join(name);
    let args = dir.join(format!("{name}.args"));
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{a}.tmp'\nmv '{a}.tmp' '{a}'\nexec sleep 60\n",
        a = args.display()
    );
    std::fs::write(&binary, script).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    (binary, args)
}

/// A Flux.1 model `id` whose weights and every component are files in `dir`.
fn flux(dir: &Path, id: u32) -> ModelLaunchSpec {
    let component = |role, name: &str| ModelComponent {
        role,
        path: file(dir, name),
    };
    ModelLaunchSpec {
        name: "flux".to_owned(),
        file_path: file(dir, "flux1-schnell-q8_0.gguf"),
        image_family: Some(ImageFamily::Flux1),
        components: vec![
            component(ComponentRole::T5xxl, "t5xxl_fp16.safetensors"),
            component(ComponentRole::Vae, "ae.safetensors"),
            component(ComponentRole::ClipL, "clip_l.safetensors"),
        ],
        file_size_bytes: 12 * GIB,
        ..launch_spec(id, "flux")
    }
}

/// `qwen` (model `id`) resident on `port`, a llama-server.
fn qwen_resident(id: u32, port: u16) -> Resident {
    Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id: id,
        model_name: "qwen".to_owned(),
        context_size: 4096,
        port,
        projector: None,
        runtime: RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 1024,
    }
}

/// A set serving `spec`, and a core whose llama-server and sd-server are
/// the binaries given.
fn set_and_core(
    spec: ModelLaunchSpec,
    base_port: u16,
    llama: &Path,
    sd: &Path,
) -> (ResidentSet, Arc<RwLock<GuiProcessCore>>) {
    let set = ResidentSet::new(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = GuiProcessCore::new(
        base_port,
        RuntimeBinaries {
            llama: llama.to_path_buf(),
            sd: sd.to_path_buf(),
        },
    );
    (set, Arc::new(RwLock::new(core)))
}

/// Start a sleeping llama-server for `qwen` (model `id`) and make it the
/// set's primary resident.
async fn running_primary(
    set: &ResidentSet,
    core: &Arc<RwLock<GuiProcessCore>>,
    dir: &Path,
    id: u32,
) {
    let weights = file(dir, "qwen.gguf");
    let config = ServerConfig::new(i64::from(id), "qwen".to_owned(), weights, 0);
    let (port, _pid) = core
        .write()
        .await
        .spawn(SpawnConfig::Llama(config))
        .await
        .expect("the stand-in starts");
    drop(set.queue().install(PRIMARY_SLOT, qwen_resident(id, port)));
}

fn incomplete_roles(error: &ModelRuntimeError) -> Vec<ComponentRole> {
    match error {
        ModelRuntimeError::ImageModelIncomplete { model, missing } => {
            assert_eq!(model, "flux");
            missing.clone()
        }
        other => panic!("expected ImageModelIncomplete, got {other:?}"),
    }
}

// --- the preflight, on its own ---

#[tokio::test]
async fn preflight_passes_an_image_model_with_everything_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let sd = file(dir.path(), "sd-server");
    assert!(preflight(&flux(dir.path(), IMAGE_ID), &sd).await.is_ok());
}

#[tokio::test]
async fn preflight_refuses_an_image_model_with_no_sd_server() {
    let dir = tempfile::tempdir().unwrap();
    let refused = preflight(&flux(dir.path(), IMAGE_ID), &dir.path().join("sd-server"))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, ModelRuntimeError::ImageRuntimeNotInstalled),
        "{refused:?}"
    );
}

/// A model that chats is not asked for sd-server.
#[tokio::test]
async fn preflight_does_not_ask_a_chat_model_for_sd_server() {
    let dir = tempfile::tempdir().unwrap();
    let chat = ModelLaunchSpec {
        file_path: file(dir.path(), "qwen.gguf"),
        ..launch_spec(3, "qwen")
    };
    assert!(
        preflight(&chat, &dir.path().join("sd-server"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn preflight_names_every_role_with_no_file_linked() {
    let dir = tempfile::tempdir().unwrap();
    let sd = file(dir.path(), "sd-server");
    let mut spec = flux(dir.path(), IMAGE_ID);
    spec.components.retain(|c| c.role == ComponentRole::ClipL);

    let refused = preflight(&spec, &sd).await.unwrap_err();

    assert_eq!(
        incomplete_roles(&refused),
        [ComponentRole::Vae, ComponentRole::T5xxl]
    );
}

#[tokio::test]
async fn preflight_names_a_linked_component_that_is_not_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let sd = file(dir.path(), "sd-server");
    let spec = flux(dir.path(), IMAGE_ID);
    let vae = dir.path().join("ae.safetensors");
    std::fs::remove_file(&vae).unwrap();

    let refused = preflight(&spec, &sd).await.unwrap_err();

    match refused {
        ModelRuntimeError::ModelFileNotFound(path) => assert_eq!(path, vae.display().to_string()),
        other => panic!("expected ModelFileNotFound, got {other:?}"),
    }
}

// --- through the admission: refused before the queue, nothing displaced ---

/// Each refusal comes before the queue: the chat model in the primary stays
/// resident and its server is never stopped.
#[tokio::test]
async fn a_refused_image_model_leaves_the_resident_running() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (llama, _) = sleeper(dir.path(), "llama-server");
    let sd = dir.path().join("sd-server");

    // Too large for any free reading, so were it queued the image model
    // would swap into the idle primary and its launch would stop the chat
    // model before failing: the refusal must come before the queue.
    let too_large = |spec: ModelLaunchSpec| ModelLaunchSpec {
        file_size_bytes: 1 << 50,
        ..spec
    };
    let not_installed = too_large(flux(dir.path(), IMAGE_ID));
    let mut incomplete = too_large(flux(dir.path(), IMAGE_ID));
    incomplete.components.clear();
    let no_vae = ModelLaunchSpec {
        file_size_bytes: 1 << 50,
        components: vec![ModelComponent {
            role: ComponentRole::Vae,
            path: dir.path().join("gone.safetensors"),
        }]
        .into_iter()
        .chain(
            flux(dir.path(), IMAGE_ID)
                .components
                .into_iter()
                .filter(|c| c.role != ComponentRole::Vae),
        )
        .collect(),
        ..flux(dir.path(), IMAGE_ID)
    };

    for (case, spec, installed) in [
        ("not installed", not_installed, false),
        ("incomplete", incomplete, true),
        ("missing VAE", no_vae, true),
    ] {
        if installed {
            file(dir.path(), "sd-server");
        }
        let (set, core) = set_and_core(spec, 19_470, &llama, &sd);
        running_primary(&set, &core, dir.path(), RESIDENT_ID).await;

        let refused = set
            .admit(&core, "flux", None, None, LaunchOverrides::default())
            .await
            .map(|_| ())
            .unwrap_err();

        match (case, &refused) {
            ("not installed", ModelRuntimeError::ImageRuntimeNotInstalled)
            | ("incomplete", ModelRuntimeError::ImageModelIncomplete { .. })
            | ("missing VAE", ModelRuntimeError::ModelFileNotFound(_)) => {}
            _ => panic!("{case}: refused with {refused:?}"),
        }
        let primary = set.queue().slot(PRIMARY_SLOT).expect("still resident");
        assert_eq!(primary.model_id, RESIDENT_ID, "{case}");
        let mut core_w = core.write().await;
        assert!(
            core_w.is_running(RESIDENT_ID),
            "{case}: the resident was stopped"
        );
        assert!(
            !core_w.has_exited(RESIDENT_ID),
            "{case}: the resident was stopped"
        );
        core_w.kill(RESIDENT_ID).await.unwrap();
    }
}

// --- the launch stops what it displaces before anything can refuse ---

/// The queue grants the launch and forgets the resident; the launched
/// model's weights are then gone. The launch fails on the file, and the
/// displaced server has been stopped, not left running unknown to the queue.
#[tokio::test]
async fn a_launch_whose_files_vanished_still_stops_the_displaced_model() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (llama, _) = sleeper(dir.path(), "llama-server");
    let weights = file(dir.path(), "late.gguf");
    let late = ModelLaunchSpec {
        file_path: weights.clone(),
        ..launch_spec(LATE_ID, "late")
    };
    let (set, core) = set_and_core(late.clone(), 19_480, &llama, &dir.path().join("sd"));
    running_primary(&set, &core, dir.path(), DISPLACED_ID).await;

    let ticket = set.queue().enqueue("late");
    let never_fits = SecondarySlotDecision::RefuseUnknownFootprint;
    let decision = set.queue().poll(&ticket, Candidate::llama(never_fits));
    let AdmissionDecision::Launch { slot, evict } = decision else {
        panic!("expected a launch into the primary, got {decision:?}");
    };
    assert_eq!((slot, evict), (PRIMARY_SLOT, Some(DISPLACED_ID)));
    std::fs::remove_file(&weights).unwrap();

    let request = LaunchRequest {
        spec: late,
        opts: ServerConfigOptions::default(),
        context: (4096, ContextSizeSource::Explicit),
        slot: PRIMARY_SLOT,
        evict: None,
        cache_ram: CacheRamSetting::Auto,
        health_deadline_secs: 5,
    };
    let refused = set
        .launch(&core, &request, slot, evict)
        .await
        .map(|_| ())
        .unwrap_err();

    assert!(
        matches!(refused, ModelRuntimeError::ModelFileNotFound(ref p) if *p == weights.display().to_string()),
        "{refused:?}"
    );
    assert!(
        !core.read().await.is_running(DISPLACED_ID),
        "the displaced server was left running"
    );
    assert!(
        set.queue().slot(PRIMARY_SLOT).is_none(),
        "the slot is free again"
    );
}

// --- a launch that succeeds ---

/// With nothing resident, an image model goes to the second slot, never the
/// empty primary, and is recorded as sd-server's with its components and no
/// context. The stand-in records the port it was given and a fake answers
/// there as sd-server does. Asked for again, it is served where it is.
#[tokio::test]
async fn an_image_model_launches_on_sd_server_into_the_second_slot() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (sd, recorded) = sleeper(dir.path(), "sd-server");
    let spec = flux(dir.path(), PLACED_ID);
    let (set, core) = set_and_core(spec.clone(), 19_490, Path::new("/nonexistent/llama"), &sd);

    let admitting = {
        let set = Arc::new(set);
        let task_set = Arc::clone(&set);
        let task_core = Arc::clone(&core);
        let handle = tokio::spawn(async move {
            task_set
                .admit(&task_core, "flux", None, None, LaunchOverrides::default())
                .await
        });
        (set, handle)
    };
    let (set, handle) = admitting;

    let argv = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&recorded) {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the stand-in sd-server was started");
    let argv: Vec<&str> = argv.lines().collect();
    let port_at = argv
        .iter()
        .position(|a| *a == "--listen-port")
        .expect("a port");
    let port: u16 = argv[port_at + 1].parse().unwrap();
    assert_eq!(argv[0], "--diffusion-model");
    assert_eq!(argv[1], spec.file_path.to_string_lossy());
    let _fake = FakeSdServer::serve_on(port).await;

    let admission = tokio::time::timeout(Duration::from_secs(20), handle)
        .await
        .expect("the launch finished")
        .unwrap()
        .expect("admitted");

    assert_eq!(admission.target.runtime, RuntimeKind::StableDiffusion);
    assert_eq!(admission.target.port, port);
    assert!(
        set.queue().slot(PRIMARY_SLOT).is_none(),
        "an image model never takes an empty primary"
    );
    let placed = set.queue().slot(1).expect("in the second slot");
    assert_eq!(placed.model_id, PLACED_ID);
    assert_eq!(placed.runtime, RuntimeKind::StableDiffusion);
    assert_eq!(placed.components, spec.components);
    assert_eq!(
        placed.context_size, 0,
        "an image model is fitted no context"
    );
    let narration = admission.target.narration.clone().expect("narrated");
    assert_eq!(
        narration.decision("slot").map(|d| d.value.as_str()),
        Some("secondary")
    );
    drop(admission);

    // Admitted again while resident: served from the second slot as it
    // stands, its health asked at sd-server's own path, nothing relaunched.
    std::fs::remove_file(&recorded).unwrap();
    let again = tokio::time::timeout(
        Duration::from_secs(10),
        set.admit(&core, "flux", None, None, LaunchOverrides::default()),
    )
    .await
    .expect("served without a launch")
    .expect("admitted");
    assert_eq!(again.lease.slot(), 1);
    assert!(!again.target.just_started, "served, not relaunched");
    assert_eq!(again.target.runtime, RuntimeKind::StableDiffusion);
    assert_eq!(again.target.port, port);
    assert!(!recorded.exists(), "the stand-in was not started again");
    drop(again);

    set.recycle(1, &core).await.unwrap();
    assert!(!core.read().await.is_running(PLACED_ID));
}

// --- recycling ---

fn sd_resident(components: Vec<ModelComponent>) -> Resident {
    Resident {
        runtime: RuntimeKind::StableDiffusion,
        components,
        context_size: 0,
        ..qwen_resident(IMAGE_ID, 1)
    }
}

/// An sd resident is recycled when a component file changed, whatever order
/// they are listed in, and never for a context size.
#[test]
fn an_sd_resident_is_recycled_for_a_relinked_component_and_never_for_context() {
    let dir = tempfile::tempdir().unwrap();
    let spec = flux(dir.path(), IMAGE_ID);
    let resident = sd_resident(spec.components.clone());

    let mut shuffled = spec.components.clone();
    shuffled.reverse();
    let same = super::resident_match::LaunchedAs {
        context: 32_768,
        projector: None,
        components: &shuffled,
    };
    assert!(
        !launched_differently(&resident, same),
        "context or order recycled it"
    );

    let mut relinked = spec.components;
    for c in &mut relinked {
        if c.role == ComponentRole::Vae {
            c.path = dir.path().join("other-ae.safetensors");
        }
    }
    let other_vae = super::resident_match::LaunchedAs {
        context: 0,
        projector: None,
        components: &relinked,
    };
    assert!(
        launched_differently(&resident, other_vae),
        "a relinked VAE kept it"
    );
}

// --- placement inputs ---

/// An image model is placed by its files and its family's margin, with no
/// KV cache; a model that chats has no image footprint.
#[test]
fn an_image_footprint_is_files_plus_the_family_margin() {
    let dir = tempfile::tempdir().unwrap();
    let footprint = vram::image_footprint(&flux(dir.path(), IMAGE_ID)).expect("a footprint");
    assert_eq!(footprint.weights_bytes, 12 * GIB + 7 * GIB);
    assert_eq!(footprint.kv_bytes, 0);
    assert!(vram::image_footprint(&launch_spec(3, "qwen")).is_none());
}

// --- a held primary ---

/// An image model too large for the free memory, with a run holding the
/// chat model in the primary, is refused at once as `ImageModelDoesNotFit`,
/// naming the held model; the chat model stays and its server runs.
#[tokio::test]
async fn an_image_model_behind_a_held_primary_does_not_fit() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (llama, _) = sleeper(dir.path(), "llama-server");
    let sd = file(dir.path(), "sd-server");
    let huge = ModelLaunchSpec {
        // More than any machine has free, so no reading grants it.
        file_size_bytes: 1 << 50,
        ..flux(dir.path(), IMAGE_ID)
    };
    let (set, core) = set_and_core(huge, 19_510, &llama, &sd);
    running_primary(&set, &core, dir.path(), HELD_ID).await;
    let port = set.queue().slot(PRIMARY_SLOT).unwrap().port;
    let _held = set.queue().hold(port, HELD_ID).expect("a run holds it");

    let refused = tokio::time::timeout(
        Duration::from_secs(5),
        set.admit(&core, "flux", None, None, LaunchOverrides::default()),
    )
    .await
    .expect("refused at once, not queued")
    .map(|_| ())
    .unwrap_err();

    match refused {
        ModelRuntimeError::ImageModelDoesNotFit {
            model, held_model, ..
        } => {
            assert_eq!(model, "flux");
            assert_eq!(held_model, "qwen");
        }
        other => panic!("expected ImageModelDoesNotFit, got {other:?}"),
    }
    assert_eq!(set.queue().slot(PRIMARY_SLOT).unwrap().model_id, HELD_ID);
    let mut core_w = core.write().await;
    assert!(core_w.is_running(HELD_ID));
    core_w.kill(HELD_ID).await.unwrap();
}
