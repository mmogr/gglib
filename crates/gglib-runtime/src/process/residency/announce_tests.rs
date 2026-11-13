//! Every load and unload is announced to the set's observer, whoever caused
//! it: a launch once its server is healthy; a stop, a recycle and a swap
//! before the process is killed; a render's retirement once its slot is
//! emptied. A recycle a run refuses announces nothing.
//!
//! Unix-only for the stand-in binaries; not for the behaviour.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::{
    CacheRamHealth, ComponentRole, ImageFamily, ModelComponent, ModelSamplingDefaults, RuntimeKind,
    SecondarySlotDecision,
};
use gglib_core::ports::{
    LaunchOverrides, ModelLaunchSpec, ProcessHandle, ResidencyObserver, ServerConfig,
};
use gglib_core::server_config::{ContextSizeSource, ServerConfigOptions};
use tokio::sync::RwLock;
use tokio::time::Instant;

use super::ResidentSet;
use super::launch::LaunchRequest;
use super::residency_tests::{OneModel, launch_spec};
use crate::process::admission::{AdmissionDecision, Candidate, PRIMARY_SLOT, Resident};
use crate::process::core::GuiProcessCore;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::sd::fake_server::FakeSdServer;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const STOPPED_ID: u32 = 999_501;
const RECYCLED_ID: u32 = 999_502;
const DISPLACED_ID: u32 = 999_504;
const LATE_ID: u32 = 999_505;
const PLACED_ID: u32 = 999_506;
const RENDERING_ID: u32 = 999_507;
const MOVED_ON_ID: u32 = 999_508;

/// What the observer was told, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Told {
    Loaded {
        model_id: i64,
        name: String,
        port: u16,
        pid: Option<u32>,
        runtime: RuntimeKind,
    },
    /// `unloaded`, and whether the model's process was still running as it
    /// was said: the set holds no lock on the process table at that moment,
    /// so the observer can ask it.
    Unloaded {
        model_id: u32,
        running: Option<bool>,
    },
}

/// Records what it is told, asking the process table whether an unloaded
/// model's server still runs.
struct Recorder {
    told: Mutex<Vec<Told>>,
    core: Arc<RwLock<GuiProcessCore>>,
}

impl Recorder {
    fn all(&self) -> Vec<Told> {
        self.told.lock().unwrap().clone()
    }
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("told", &self.told)
            .finish_non_exhaustive()
    }
}

impl ResidencyObserver for Recorder {
    fn loaded(&self, server: ProcessHandle) {
        self.told.lock().unwrap().push(Told::Loaded {
            model_id: server.model_id,
            name: server.model_name,
            port: server.port,
            pid: server.pid,
            runtime: server.runtime,
        });
    }

    fn unloaded(&self, model_id: u32) {
        let running = self.core.try_read().ok().map(|c| c.is_running(model_id));
        self.told
            .lock()
            .unwrap()
            .push(Told::Unloaded { model_id, running });
    }
}

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

/// A set serving `spec` over a process table whose llama-server and
/// sd-server are the binaries given, and the recorder it tells.
fn observed(
    spec: ModelLaunchSpec,
    base_port: u16,
    llama: &Path,
    sd: &Path,
) -> (ResidentSet, Arc<RwLock<GuiProcessCore>>, Arc<Recorder>) {
    let core = Arc::new(RwLock::new(GuiProcessCore::new(
        base_port,
        RuntimeBinaries {
            llama: llama.to_path_buf(),
            sd: sd.to_path_buf(),
        },
    )));
    let recorder = Arc::new(Recorder {
        told: Mutex::new(Vec::new()),
        core: Arc::clone(&core),
    });
    let set = ResidentSet::new(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        Arc::clone(&recorder) as Arc<dyn ResidencyObserver>,
    );
    (set, core, recorder)
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

/// Start a stand-in for model `id` and install it in `slot`, past the
/// launch, so nothing is announced; returns its port.
async fn running(
    set: &ResidentSet,
    core: &Arc<RwLock<GuiProcessCore>>,
    dir: &Path,
    (id, name, slot, runtime): (u32, &str, usize, RuntimeKind),
) -> u16 {
    let weights = file(dir, &format!("{name}.gguf"));
    let config = ServerConfig::new(i64::from(id), name.to_owned(), weights, 0);
    let (port, _pid) = core
        .write()
        .await
        .spawn(SpawnConfig::Llama(config))
        .expect("the stand-in starts");
    drop(set.queue().install(slot, resident(id, name, port, runtime)));
    port
}

fn unloaded_while_running(model_id: u32) -> Told {
    Told::Unloaded {
        model_id,
        running: Some(true),
    }
}

#[tokio::test]
async fn a_stop_announces_the_model_unloaded_before_its_server_is_stopped() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (binary, _) = sleeper(dir.path(), "server");
    let (set, core, told) = observed(launch_spec(STOPPED_ID, "qwen"), 19_560, &binary, &binary);
    running(
        &set,
        &core,
        dir.path(),
        (STOPPED_ID, "qwen", PRIMARY_SLOT, RuntimeKind::Llama),
    )
    .await;

    assert!(set.stop_model(STOPPED_ID, &core).await.unwrap());

    assert_eq!(told.all(), [unloaded_while_running(STOPPED_ID)]);
    assert!(!core.read().await.is_running(STOPPED_ID));

    // Stopped again: nothing was running, so nothing is announced.
    assert!(!set.stop_model(STOPPED_ID, &core).await.unwrap());
    assert_eq!(told.all().len(), 1);
}

#[tokio::test]
async fn a_recycle_announces_the_model_unloaded() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (binary, _) = sleeper(dir.path(), "server");
    let (set, core, told) = observed(launch_spec(RECYCLED_ID, "qwen"), 19_570, &binary, &binary);
    running(
        &set,
        &core,
        dir.path(),
        (RECYCLED_ID, "qwen", PRIMARY_SLOT, RuntimeKind::Llama),
    )
    .await;

    set.recycle(PRIMARY_SLOT, &core).await;

    assert_eq!(told.all(), [unloaded_while_running(RECYCLED_ID)]);
    assert!(!core.read().await.is_running(RECYCLED_ID));
}

/// The queue granted the launch and forgot the resident; the launch then
/// fails on its files. The displaced model's unload was announced before
/// its server was stopped, and no load is announced.
#[tokio::test]
async fn a_swap_announces_the_displaced_model_unloaded_before_it_is_stopped() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (binary, _) = sleeper(dir.path(), "server");
    let weights = file(dir.path(), "late.gguf");
    let late = ModelLaunchSpec {
        file_path: weights.clone(),
        ..launch_spec(LATE_ID, "late")
    };
    let (set, core, told) = observed(late.clone(), 19_580, &binary, &binary);
    running(
        &set,
        &core,
        dir.path(),
        (DISPLACED_ID, "qwen", PRIMARY_SLOT, RuntimeKind::Llama),
    )
    .await;

    let ticket = set.queue().enqueue(LATE_ID, "late");
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
        fit: None,
        fit_off: false,
    };

    let refused = set.launch(&core, &request, slot, evict).await;

    assert!(refused.is_err(), "the launch found no weights");
    assert_eq!(told.all(), [unloaded_while_running(DISPLACED_ID)]);
    assert!(!core.read().await.is_running(DISPLACED_ID));
}

/// A launch is announced once its server answers its health check, by the
/// handle `list_running` reports: id, name, port, pid and runtime. The
/// stand-in sd-server records the port it was given, and a fake answers
/// there as sd-server does, only once the launch is seen waiting.
#[tokio::test]
async fn a_launched_model_is_announced_loaded_once_healthy_with_its_port_and_pid() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (sd, recorded) = sleeper(dir.path(), "sd-server");
    let component = |role, name: &str| ModelComponent {
        role,
        path: file(dir.path(), name),
    };
    let spec = ModelLaunchSpec {
        name: "flux".to_owned(),
        file_path: file(dir.path(), "flux1-schnell-q8_0.gguf"),
        image_family: Some(ImageFamily::Flux1),
        components: vec![
            component(ComponentRole::T5xxl, "t5xxl_fp16.safetensors"),
            component(ComponentRole::Vae, "ae.safetensors"),
            component(ComponentRole::ClipL, "clip_l.safetensors"),
        ],
        file_size_bytes: 12 << 30,
        ..launch_spec(PLACED_ID, "flux")
    };
    let (set, core, told) = observed(spec, 19_590, Path::new("/nonexistent/llama"), &sd);
    let set = Arc::new(set);

    let handle = {
        let (set, core) = (Arc::clone(&set), Arc::clone(&core));
        tokio::spawn(async move {
            set.admit(&core, "flux", None, None, LaunchOverrides::default())
                .await
        })
    };
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
    assert_eq!(
        told.all(),
        [],
        "nothing is announced before the health check"
    );
    let _fake = FakeSdServer::serve_on(port).await;

    let admission = tokio::time::timeout(Duration::from_secs(20), handle)
        .await
        .expect("the launch finished")
        .unwrap()
        .expect("admitted");

    let pid = core
        .read()
        .await
        .list_all()
        .into_iter()
        .find(|info| info.model_id == PLACED_ID)
        .map(|info| info.pid);
    assert!(pid.is_some(), "the process table knows the server");
    assert_eq!(
        told.all(),
        [Told::Loaded {
            model_id: i64::from(PLACED_ID),
            name: "flux".to_owned(),
            port,
            pid,
            runtime: RuntimeKind::StableDiffusion,
        }]
    );
    assert_eq!(admission.target.port, port);
    drop(admission);

    set.recycle(1, &core).await;
    assert_eq!(told.all().last(), Some(&unloaded_while_running(PLACED_ID)));
}

/// A render's retirement kills first and empties the slot after, so its
/// unload is announced once the server is gone.
#[tokio::test]
async fn a_retired_render_announces_its_model_unloaded() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (binary, _) = sleeper(dir.path(), "server");
    let (set, core, told) = observed(launch_spec(RENDERING_ID, "flux"), 19_600, &binary, &binary);
    running(
        &set,
        &core,
        dir.path(),
        (RENDERING_ID, "flux", 1, RuntimeKind::StableDiffusion),
    )
    .await;
    let queue = Arc::clone(set.queue());
    let lease = queue.lease(1).expect("the render's lease");
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");
    let kill = async {
        core.write().await.kill(RENDERING_ID).await.unwrap();
    };

    let retired = set.retire_render(turn, RENDERING_ID, kill).await;

    assert_eq!(retired.map(|r| r.model_id), Some(RENDERING_ID));
    assert_eq!(
        told.all(),
        [Told::Unloaded {
            model_id: RENDERING_ID,
            running: Some(false),
        }]
    );
    assert!(queue.slot(1).is_none());
}

/// A render whose slot was emptied under it before it retired (a Stop or a
/// recycle got there first, and announced the unload then) retires nothing
/// and announces nothing: the unload is said once.
#[tokio::test]
async fn a_retire_whose_slot_moved_on_announces_nothing() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let (binary, _) = sleeper(dir.path(), "server");
    let (set, core, told) = observed(launch_spec(MOVED_ON_ID, "flux"), 19_610, &binary, &binary);
    running(
        &set,
        &core,
        dir.path(),
        (MOVED_ON_ID, "flux", 1, RuntimeKind::StableDiffusion),
    )
    .await;
    let queue = Arc::clone(set.queue());
    let lease = queue.lease(1).expect("the render's lease");
    let turn = queue
        .generation_gate()
        .render_turn(lease, None)
        .await
        .expect("nothing else generates");
    // Emptied past the set, so nothing is announced for it here.
    assert!(queue.evict(1).is_some());
    let kill = async {
        core.write().await.kill(MOVED_ON_ID).await.unwrap();
    };

    let retired = set.retire_render(turn, MOVED_ON_ID, kill).await;

    assert!(retired.is_none());
    assert_eq!(told.all(), []);
    assert!(!core.read().await.is_running(MOVED_ON_ID));
}
