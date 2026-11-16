//! [`super::ProcessManager::kill_all_now`]: the stop for a service that has
//! seconds left, as on Windows after its console window is closed.
//!
//! The case it exists for is the one the orderly stop is slowest in: an image
//! model drawing in the second slot, beside a chat model in the primary. The
//! orderly stop asks the render to let go and waits up to
//! [`RENDER_STOP_WAIT`](crate::process::residency::RENDER_STOP_WAIT) for it,
//! longer than Windows lets a closing console's process live.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{
    CacheRamHealth, ComponentRole, ImageFamily, ModelComponent, ModelSamplingDefaults, RuntimeKind,
};
use gglib_core::ports::{NoopResidencyObserver, ServerConfig};
use gglib_core::server_config::ServerConfigOptions;

use super::ProcessManager;
use crate::process::admission::{PRIMARY_SLOT, Resident};
use crate::process::core::drop_tests::{clean_up, is_running, stops};
use crate::process::residency::residency_tests::StubCatalog;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::sd::SdServerConfig;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const CHAT_ID: u32 = 999_330;
const IMAGE_ID: u32 = 999_331;
const BASE_PORT: u16 = 19_430;
/// The image model's slot, beside the chat model.
const SECOND_SLOT: usize = 1;

/// How long the stop may take: well inside the five seconds or so Windows
/// gives a process whose console is closing.
const HURRIED: Duration = Duration::from_secs(2);

/// A stand-in server called `name` that sleeps until it is stopped.
fn sleeper(dir: &Path, name: &str) -> PathBuf {
    gglib_test_proc::install(dir, name, "sleep 60")
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
        resident_since: tokio::time::Instant::now(),
        weights_bytes: 1024,
    }
}

/// A chat model's llama-server in the primary slot and an image model's
/// `sd-server` in the second, drawing: its render holds the turn and nothing
/// will ever let it go. The call returns within two seconds, and every
/// engine's process is gone.
#[tokio::test]
async fn every_engine_is_killed_at_once_even_beside_a_render() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let weights = dir.path().join("model.gguf");
    std::fs::write(&weights, b"not really a gguf").expect("write model file");
    let manager = ProcessManager::new(
        BASE_PORT,
        RuntimeBinaries {
            llama: sleeper(dir.path(), "llama-server"),
            sd: sleeper(dir.path(), "sd-server"),
        },
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        Arc::new(NoopResidencyObserver),
    );

    let chat = ServerConfig::new(
        i64::from(CHAT_ID),
        "qwen".to_owned(),
        weights.clone(),
        BASE_PORT,
    );
    let (chat_port, chat_pid) = manager
        .core
        .write()
        .await
        .spawn(SpawnConfig::Llama(chat))
        .expect("the chat stand-in starts");
    let queue = Arc::clone(manager.residency.queue());
    drop(queue.install(
        PRIMARY_SLOT,
        resident(CHAT_ID, "qwen", chat_port, RuntimeKind::Llama),
    ));

    let image = SdServerConfig {
        model_id: i64::from(IMAGE_ID),
        model_name: "flux".to_owned(),
        model_path: weights,
        family: ImageFamily::Flux1,
        components: vec![ModelComponent {
            role: ComponentRole::Vae,
            path: PathBuf::from("/models/ae.safetensors"),
        }],
        port: None,
    };
    let (image_port, image_pid) = manager
        .core
        .write()
        .await
        .spawn(SpawnConfig::Sd(image))
        .expect("the image stand-in starts");
    let lease = queue.install(
        SECOND_SLOT,
        resident(IMAGE_ID, "flux", image_port, RuntimeKind::StableDiffusion),
    );
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");
    let ran = [is_running(chat_pid), is_running(image_pid)];

    let asked = Instant::now();
    let returned = tokio::time::timeout(HURRIED, manager.kill_all_now()).await;
    let took = asked.elapsed();
    let stopped = [stops(chat_pid).await, stops(image_pid).await];

    drop(turn);
    clean_up(i64::from(CHAT_ID), chat_pid, stopped[0]);
    clean_up(i64::from(IMAGE_ID), image_pid, stopped[1]);
    assert_eq!(ran, [true, true], "both engines must run before the stop");
    assert!(returned.is_ok(), "the stop had not returned after {took:?}");
    assert_eq!(
        stopped,
        [true, true],
        "every engine must be stopped, the drawing one too"
    );
}
