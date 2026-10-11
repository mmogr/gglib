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
