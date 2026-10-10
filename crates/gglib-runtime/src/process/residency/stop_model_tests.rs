//! A person's Stop reaches the model they chose, in whichever slot it sits:
//! an image model beside a chat model is stopped and its slot emptied, and
//! the chat model keeps running.
//!
//! Unix-only for the stand-in binaries; not for the behaviour.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{CacheRamHealth, ModelSamplingDefaults, RuntimeKind};
use gglib_core::ports::ServerConfig;
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;
use tokio::time::Instant;

use super::ResidentSet;
use super::residency_tests::{OneModel, launch_spec};
use crate::process::admission::{PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::{RuntimeBinaries, SpawnConfig};

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const CHAT_ID: u32 = 999_301;
const IMAGE_ID: u32 = 999_302;
const ELSEWHERE_ID: u32 = 999_303;
const RENDERING_ID: u32 = 999_304;

/// A stand-in server that sleeps until it is killed.
fn sleeper(dir: &Path) -> PathBuf {
    let binary = dir.join("server");
    std::fs::write(&binary, "#!/bin/sh\nexec sleep 60\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    binary
}

fn resident(model_id: u32, name: &str, port: u16, runtime: RuntimeKind) -> Resident {
    Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id,
        model_name: name.to_owned(),
        context_size: 0,
        port,
        projector: None,
        runtime,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 1024,
    }
}

/// Start a stand-in for model `id` and install it in `slot`.
async fn running(
    set: &ResidentSet,
    core: &Arc<RwLock<GuiProcessCore>>,
    dir: &Path,
    (id, name, slot, runtime): (u32, &str, usize, RuntimeKind),
) {
    let weights = dir.join(format!("{name}.gguf"));
    std::fs::write(&weights, b"x").unwrap();
    let config = ServerConfig::new(i64::from(id), name.to_owned(), weights, 0);
    let (port, _pid) = core
        .write()
        .await
        .spawn(SpawnConfig::Llama(config))
        .await
        .expect("the stand-in starts");
    drop(set.queue().install(slot, resident(id, name, port, runtime)));
}

#[tokio::test]
async fn stopping_the_image_model_beside_a_chat_model_leaves_the_chat_model_running() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let binary = sleeper(dir.path());
    let set = ResidentSet::new(
        Arc::new(OneModel(launch_spec(CHAT_ID, "qwen"))),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_490,
        RuntimeBinaries {
            llama: binary.clone(),
            sd: binary,
        },
    )));
    running(
        &set,
        &core,
        dir.path(),
        (CHAT_ID, "qwen", PRIMARY_SLOT, RuntimeKind::Llama),
    )
    .await;
    let image = (IMAGE_ID, "flux", 1, RuntimeKind::StableDiffusion);
    running(&set, &core, dir.path(), image).await;

    assert!(
        !set.stop_model(ELSEWHERE_ID, &core).await.unwrap(),
        "not running here"
    );
    assert!(set.stop_model(IMAGE_ID, &core).await.unwrap());

    assert!(
        set.queue().slot(1).is_none(),
        "the image model's slot is empty"
    );
    let primary = set
        .queue()
        .slot(PRIMARY_SLOT)
        .expect("the chat model stays");
    assert_eq!(primary.model_id, CHAT_ID);
    let core_r = core.read().await;
    assert!(
        !core_r.is_running(IMAGE_ID),
        "the image model's server was stopped"
    );
    assert!(
        core_r.is_running(CHAT_ID),
        "the chat model's server was not"
    );
    drop(core_r);

    assert!(
        !set.stop_model(IMAGE_ID, &core).await.unwrap(),
        "already stopped"
    );
    assert!(set.stop_model(CHAT_ID, &core).await.unwrap());
    assert!(set.queue().primary().is_none());
}

/// A Stop on an image model that is drawing waits for the render to retire
/// it, never emptying the slot under the render's lease: a model launched
/// into the slot after the Stop keeps its own request when the old render
/// goes away.
#[tokio::test]
async fn stopping_an_image_model_mid_render_leaves_the_slots_next_model_its_request() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let binary = sleeper(dir.path());
    let set = ResidentSet::new(
        Arc::new(OneModel(launch_spec(CHAT_ID, "qwen"))),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_470,
        RuntimeBinaries {
            llama: binary.clone(),
            sd: binary,
        },
    )));
    let image = (RENDERING_ID, "flux", 1, RuntimeKind::StableDiffusion);
    running(&set, &core, dir.path(), image).await;
    let queue = Arc::clone(set.queue());
    let lease = queue.lease(1).expect("the render's lease");
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");

    // The render's driver: it holds the turn, and retires its model once a
    // Stop was asked, as `sd::job_poll` does at each read of its job.
    let driver = {
        let (queue, core) = (Arc::clone(&queue), Arc::clone(&core));
        tokio::spawn(async move {
            while !queue.render_stop_asked(RENDERING_ID) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            let held = queue.slot(1).map(|r| (r.model_id, r.inflight));
            let kill = async {
                core.write().await.kill(RENDERING_ID).await.unwrap();
            };
            queue.retire_render(turn, RENDERING_ID, kill).await;
            held
        })
    };

    assert!(set.stop_model(RENDERING_ID, &core).await.unwrap());

    assert!(queue.slot(1).is_none(), "the image model's slot is empty");
    assert!(!core.read().await.is_running(RENDERING_ID));
    let newcomer = queue.install(1, resident(CHAT_ID, "qwen", 1, RuntimeKind::Llama));
    // Whatever the old render still held goes now.
    driver.abort();
    let held = driver.await.ok();
    assert_eq!(
        queue.slot(1).map(|r| r.inflight),
        Some(1),
        "the newcomer keeps its request"
    );
    assert_eq!(
        held,
        Some(Some((RENDERING_ID, 1))),
        "the render still held its slot and its count when it was asked"
    );
    drop(newcomer);
}

/// A Stop on an image model whose render never lets go gives up after
/// `RENDER_STOP_WAIT` with an error, and has emptied nothing: the slot still
/// holds the model with the render's request counted, and the render stays
/// asked, to stop when its driver next reads its job.
#[tokio::test(start_paused = true)]
async fn a_stop_that_outwaits_a_render_answers_an_error_and_empties_nothing() {
    const STUCK_ID: u32 = 999_305;
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("never-run");
    let set = ResidentSet::new(
        Arc::new(OneModel(launch_spec(CHAT_ID, "qwen"))),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_460,
        RuntimeBinaries {
            llama: binary.clone(),
            sd: binary,
        },
    )));
    let queue = Arc::clone(set.queue());
    // No process: the Stop must give up before it reaches for one.
    let lease = queue.install(
        1,
        resident(STUCK_ID, "flux", 1, RuntimeKind::StableDiffusion),
    );
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");

    let asked_at = Instant::now();
    // Bounded, so a Stop that never gives up fails here instead of hanging.
    let stopped =
        tokio::time::timeout(super::RENDER_STOP_WAIT * 3, set.stop_model(STUCK_ID, &core))
            .await
            .expect("the Stop gives up on its own");

    let waited = asked_at.elapsed();
    assert!(
        matches!(
            stopped,
            Err(gglib_core::ports::ModelRuntimeError::Internal(_))
        ),
        "{stopped:?}"
    );
    assert!(
        waited >= super::RENDER_STOP_WAIT && waited < super::RENDER_STOP_WAIT * 2,
        "gave up after {waited:?}"
    );
    assert_eq!(
        queue.slot(1).map(|r| (r.model_id, r.inflight)),
        Some((STUCK_ID, 1)),
        "the slot and the render's request are as they were"
    );
    assert!(queue.render_stop_asked(STUCK_ID), "the render stays asked");
    drop(turn);
}

/// Stopping the current model goes the same way when it is an image model
/// that swapped into the primary slot and is drawing: the render is asked
/// and waited for, and its slot is never emptied under it.
#[tokio::test(start_paused = true)]
async fn stopping_the_current_model_asks_a_render_in_the_primary_slot_too() {
    const PRIMARY_IMAGE_ID: u32 = 999_306;
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("never-run");
    let set = ResidentSet::new(
        Arc::new(OneModel(launch_spec(CHAT_ID, "qwen"))),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        19_450,
        RuntimeBinaries {
            llama: binary.clone(),
            sd: binary,
        },
    )));
    let queue = Arc::clone(set.queue());
    let image = resident(PRIMARY_IMAGE_ID, "flux", 1, RuntimeKind::StableDiffusion);
    let lease = queue.install(PRIMARY_SLOT, image);
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");

    let stopped = tokio::time::timeout(super::RENDER_STOP_WAIT * 3, set.stop_primary(&core))
        .await
        .expect("the Stop gives up on its own");

    assert!(stopped.is_err(), "the render never let go: {stopped:?}");
    assert!(queue.render_stop_asked(PRIMARY_IMAGE_ID), "it was asked");
    assert_eq!(
        queue.primary().map(|r| (r.model_id, r.inflight)),
        Some((PRIMARY_IMAGE_ID, 1)),
        "the slot was not emptied under the render"
    );
    drop(turn);
}
