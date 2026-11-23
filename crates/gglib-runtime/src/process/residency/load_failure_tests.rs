//! A model that cannot load says why (#1959 C5): an engine already found
//! not working is refused before anything is started, with the reason its
//! status gives; a server that stops while it loads, or is never ready, is
//! reported with what it printed and how it ended.
//!
//! Each engine is a `gglib-test-proc` stand-in, which runs its whole script
//! whatever it is asked, `--version` included.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{
    LaunchOverrides, LoadCause, ModelLaunchSpec, ModelRuntimeError, NoopResidencyObserver,
};
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;

use super::residency_tests::{OneModel, launch_spec};
use super::{ResidentSet, prepare};
use crate::process::RuntimeBinaries;
use crate::process::admission::AdmissionQueue;
use crate::process::core::GuiProcessCore;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const REFUSED_ID: u32 = 999_311;
const MENDED_ID: u32 = 999_312;
const STOPPED_ID: u32 = 999_313;
const NEVER_READY_ID: u32 = 999_314;
const REFUSED_AGAIN_ID: u32 = 999_315;
const RESIDENT_ID: u32 = 999_316;
const REAPED_ID: u32 = 999_317;
const OTHER_ID: u32 = 999_318;

/// What glibc's loader prints for a library it cannot find, before it
/// exits 127.
const LOADER_LINE: &str = "./llama-server: error while loading shared libraries: \
                           libvulkan.so.1: cannot open shared object file: No such file or directory";

/// What llama-server (build 12110) wrote on stderr when it was asked to
/// load a file that is not a GGUF, before it exited 1; the path shortened.
const NOT_A_GGUF: &[&str] = &[
    "0.00.003.290 I srv  llama_server: initializing ...",
    "0.00.123.640 W srv  server_setup: security: no API key is set and CORS allows all origins",
    "0.00.125.084 I srv    load_model: loading model '/models/qwen.gguf'",
    "0.00.125.192 E gguf_init_from_reader: invalid magic characters: 'this', expected 'GGUF'",
    "0.00.125.716 E llama_model_load: error loading model: llama_model_loader: failed to load model from /models/qwen.gguf",
    "0.00.125.833 E llama_model_load_from_file_impl: failed to load model",
    "0.00.126.304 E srv    load_model: failed to load model, '/models/qwen.gguf'",
    "0.00.126.305 I srv    operator(): operator(): cleaning up before exit...",
    "0.00.126.796 E srv  server_run_i: exiting due to model loading error",
];

/// The model `id`, `qwen`, whose weights are a file in `dir`.
fn model(dir: &Path, id: u32) -> ModelLaunchSpec {
    let weights = dir.join("qwen.gguf");
    std::fs::write(&weights, b"this is not a gguf").expect("write the weights");
    ModelLaunchSpec {
        file_path: weights,
        file_size_bytes: 1024,
        ..launch_spec(id, "qwen")
    }
}

/// A set serving `spec`, and a core whose llama-server is `llama`.
fn set_and_core(
    spec: ModelLaunchSpec,
    llama: &Path,
    base_port: u16,
) -> (ResidentSet, Arc<RwLock<GuiProcessCore>>) {
    let set = ResidentSet::unobserved(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    let core = GuiProcessCore::new(
        base_port,
        RuntimeBinaries::llama_only(llama.to_string_lossy()),
    );
    (set, Arc::new(RwLock::new(core)))
}

/// What the stand-in at `argv` was last run with, one argument a line;
/// empty when it never ran.
fn last_run(argv: &Path) -> Vec<String> {
    std::fs::read_to_string(argv)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// A stand-in llama-server in `dir` that records its arguments and then
/// follows `script`.
fn engine(dir: &Path, script: &str) -> (PathBuf, PathBuf) {
    let argv = dir.join("llama-server.argv");
    let binary = gglib_test_proc::install(
        dir,
        "llama-server",
        &format!("argv {}\n{script}", argv.display()),
    );
    (binary, argv)
}

/// `lines`, each written on stderr.
fn eprints(lines: &[&str]) -> String {
    lines.iter().fold(String::new(), |mut script, line| {
        script.push_str("eprint ");
        script.push_str(line);
        script.push('\n');
        script
    })
}

/// An engine its status has found not working is refused before anything
/// is started, with the reason the status gives: the one check's words.
#[tokio::test]
async fn an_engine_known_not_to_work_is_refused_before_anything_starts() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let (llama, argv) = engine(dir.path(), &format!("eprint {LOADER_LINE}\nexit 127"));
    // What Settings › Engines and the setup wizard ask.
    let reason = crate::engine::validate_engine_at(RuntimeKind::Llama, &llama)
        .expect_err("an engine that cannot start is not working")
        .to_string();
    let (set, core) = set_and_core(model(dir.path(), REFUSED_ID), &llama, 19_700);

    let refusal = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("an engine known not to work is refused");

    let ran = last_run(&argv);
    core.write()
        .await
        .kill(REFUSED_ID, crate::process::StopReason::Asked)
        .await
        .ok();
    // The refusal's kind is what says nothing was started: a server that
    // was started and then exited would be `Stopped`, whenever it is read.
    let ModelRuntimeError::LoadFailed(failure) = &refusal else {
        panic!("a failed load: {refusal:?}");
    };
    assert!(
        matches!(failure.cause, LoadCause::EngineNotWorking { .. }),
        "refused before a start: {failure:?}"
    );
    assert_eq!(refusal.to_string(), reason, "the status's own sentence");
    assert!(
        !ran.iter().any(|arg| arg == "-m"),
        "no serving run: {ran:?}"
    );
}

/// The refusal comes before the queue, so a model already loaded keeps
/// running: a launch refused after its stop of the resident would have
/// stopped it for nothing.
#[tokio::test]
async fn a_refusal_leaves_the_loaded_model_running() {
    use crate::process::admission::PRIMARY_SLOT;

    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    // One binary: running and sleeping for the resident, then found unable
    // to start, as a library taken away from under a running server is.
    let (llama, argv) = engine(dir.path(), "sleep 60");
    let (set, core) = set_and_core(model(dir.path(), REFUSED_AGAIN_ID), &llama, 19_740);
    let resident_model = dir.path().join("resident.gguf");
    std::fs::write(&resident_model, b"x").expect("the resident's weights");
    let config = gglib_core::ports::ServerConfig::new(
        i64::from(RESIDENT_ID),
        "resident".to_owned(),
        resident_model,
        0,
    );
    let (port, _) = core
        .write()
        .await
        .spawn(crate::process::SpawnConfig::Llama(config))
        .expect("the resident starts");
    drop(
        set.queue()
            .install(PRIMARY_SLOT, resident(RESIDENT_ID, port)),
    );
    // It has read its script, and is sleeping, once it has written its
    // arguments: only then is the script changed under it.
    for _ in 0..500 {
        if last_run(&argv).iter().any(|arg| arg == "-m") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    std::fs::write(
        gglib_test_proc::instructions_path(&llama),
        format!("eprint {LOADER_LINE}\nexit 127\n"),
    )
    .expect("break the engine");
    crate::engine::validate_engine_at(RuntimeKind::Llama, &llama)
        .expect_err("an engine that cannot start is not working");

    let refusal = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("refused");

    let still_running = !core.write().await.has_exited(RESIDENT_ID);
    let still_resident = set
        .queue()
        .residents()
        .iter()
        .any(|(_, r)| r.model_id == RESIDENT_ID);
    core.write()
        .await
        .kill(RESIDENT_ID, crate::process::StopReason::Asked)
        .await
        .ok();
    assert!(
        matches!(refusal, ModelRuntimeError::LoadFailed(_)),
        "{refusal:?}"
    );
    assert!(still_running, "the loaded model's server was stopped");
    assert!(still_resident, "the loaded model was forgotten");
}

/// A resident `id` on `port`, a llama-server.
fn resident(id: u32, port: u16) -> crate::process::admission::Resident {
    crate::process::admission::Resident {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        model_id: id,
        model_name: "resident".to_owned(),
        context_size: 4096,
        port,
        projector: None,
        runtime: RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: gglib_core::domain::CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: tokio::time::Instant::now(),
        weights_bytes: 1024,
    }
}

/// An engine whose `--version` only timed out (a scanner on a fresh binary,
/// a loaded machine) is not remembered as unable to start, so its load is
/// not refused; one that cannot start is.
#[test]
fn only_an_engine_that_cannot_start_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let slow = gglib_test_proc::install(dir.path(), "slow-server", "sleep 60");
    let broken = gglib_test_proc::install(
        dir.path(),
        "broken-server",
        &format!("eprint {LOADER_LINE}\nexit 127"),
    );
    let install = gglib_core::paths::LLAMA_INSTALL_COMMAND;

    let timed_out = crate::engine::validate_binary(&slow, install, Duration::from_millis(500))
        .expect_err("it did not answer in time");
    crate::engine::validate_binary(&broken, install, Duration::from_secs(10))
        .expect_err("it cannot start");

    assert_eq!(
        timed_out.failure,
        gglib_core::utils::process::bounded::Failure::TimedOut
    );
    // Not remembered, so not even asked again: the answer is at once.
    let asked = std::time::Instant::now();
    assert!(crate::engine::refuse_if_known_not_working(RuntimeKind::Llama, &slow).is_ok());
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "{:?}",
        asked.elapsed()
    );
    assert!(crate::engine::refuse_if_known_not_working(RuntimeKind::Llama, &broken).is_err());

    // One that could not start and now only times out, asked again, is let
    // through to its load.
    std::fs::write(gglib_test_proc::instructions_path(&broken), "sleep 60\n")
        .expect("make it slow");
    assert!(crate::engine::refuse_if_known_not_working(RuntimeKind::Llama, &broken).is_ok());
}

/// The launch reads a server's output once its readers are done: a server
/// that printed why and exited is seen to exit before its last lines are
/// read, and they are still the reason given.
#[tokio::test]
async fn a_server_that_exited_is_read_to_the_end_of_its_output() {
    let output = crate::process::OutputTail::default();
    output.opened();
    let reader = output.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        reader.push("0.00.100.000 E gguf_init_from_file: failed to open GGUF file");
        reader.closed();
    });

    let cause =
        super::launch::launch_failure::stopped(crate::process::core::Exit::Unknown, &output).await;

    assert_eq!(
        cause,
        LoadCause::Stopped {
            failure: None,
            said: Some("gguf_init_from_file: failed to open GGUF file".to_owned()),
        }
    );
}

/// How a server ended is kept when another launch's cleanup takes it from
/// the core first: two servers, one exits, a second launch cleans up, and
/// the first launch still reads its status.
#[tokio::test]
async fn an_exit_reaped_by_another_launch_is_still_read() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let (llama, _) = engine(dir.path(), "exit 3");
    let sleeper = gglib_test_proc::install(dir.path(), "sd-server", "sleep 60");
    let weights = dir.path().join("w.gguf");
    std::fs::write(&weights, b"x").expect("weights");
    let mut core = GuiProcessCore::new(
        19_750,
        RuntimeBinaries {
            llama: llama.clone(),
            sd: sleeper,
        },
    );
    let chat = gglib_core::ports::ServerConfig::new(
        i64::from(REAPED_ID),
        "chat".to_owned(),
        weights.clone(),
        0,
    );
    core.spawn(crate::process::SpawnConfig::Llama(chat))
        .expect("the chat server starts");
    let image = crate::sd::SdServerConfig {
        model_id: i64::from(OTHER_ID),
        model_name: "flux".to_owned(),
        model_path: weights,
        family: gglib_core::domain::ImageFamily::Flux1,
        components: Vec::new(),
        port: None,
    };
    core.spawn(crate::process::SpawnConfig::Sd(image))
        .expect("the image server starts");
    // The second launch's cleanup, run until it has taken the exited one.
    for _ in 0..250 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        if core.cleanup_dead().contains(&REAPED_ID) {
            break;
        }
    }

    let exit = core.exited(REAPED_ID);
    let other_running = !core.has_exited(OTHER_ID);
    core.kill(OTHER_ID, crate::process::StopReason::Asked)
        .await
        .ok();

    assert!(
        matches!(exit, Some(crate::process::core::Exit::Status(status)) if status.code() == Some(3)),
        "{exit:?}"
    );
    assert!(other_running, "the other server is left alone");
}

/// A refusal is not remembered past a mend: an engine found not working
/// and since put right (a library installed, say, which leaves the binary
/// as it was) is asked again and started.
#[tokio::test]
async fn an_engine_mended_since_it_was_found_not_working_is_started() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let (llama, argv) = engine(dir.path(), &format!("eprint {LOADER_LINE}\nexit 127"));
    let reason = crate::engine::validate_engine_at(RuntimeKind::Llama, &llama)
        .expect_err("an engine that cannot start is not working")
        .to_string();
    // The same binary, mended: it now answers and exits 0.
    std::fs::write(
        gglib_test_proc::instructions_path(&llama),
        format!("argv {}\nprint version: 1 (test)\n", argv.display()),
    )
    .expect("mend the stand-in");
    let (set, core) = set_and_core(model(dir.path(), MENDED_ID), &llama, 19_710);

    let failure = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("the stand-in exits at once, so the load still fails")
        .to_string();

    let ran = last_run(&argv);
    core.write()
        .await
        .kill(MENDED_ID, crate::process::StopReason::Asked)
        .await
        .ok();
    assert!(
        ran.iter().any(|arg| arg == "--port"),
        "a mended engine is started: {ran:?}"
    );
    assert!(!failure.contains(&reason), "{failure}");
}

/// A server that prints why on stderr and exits before it is ready: the
/// load's error carries the first line its engine marked as an error, and
/// how it ended, in the words the engine check uses.
#[tokio::test]
async fn a_server_that_stops_while_loading_says_why() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let (llama, _) = engine(dir.path(), &format!("{}exit 1", eprints(NOT_A_GGUF)));
    let (set, core) = set_and_core(model(dir.path(), STOPPED_ID), &llama, 19_720);
    let asked = std::time::Instant::now();

    let failure = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("a server that exits does not load")
        .to_string();

    core.write()
        .await
        .kill(STOPPED_ID, crate::process::StopReason::Asked)
        .await
        .ok();
    // Kept for this model, by its id, as it failed: what the daemon's proof
    // reads for `gglib up`. No other model's is.
    let kept = set.last_load_failure(STOPPED_ID).expect("kept");
    assert!(kept.0 >= asked);
    assert_eq!(kept.1.sentence(), failure);
    assert!(set.last_load_failure(NEVER_READY_ID).is_none());
    assert!(
        failure.contains("invalid magic characters: 'this', expected 'GGUF'"),
        "{failure}"
    );
    assert!(failure.contains("it exited with status 1"), "{failure}");
    assert!(failure.contains("qwen"), "{failure}");
    assert!(
        !failure.contains("0.00.125.192"),
        "the engine's timestamp is not the reason: {failure}"
    );
}

/// A server still loading when its wait is up: the error says it was not
/// ready in time, and gives the last line it printed.
#[tokio::test]
async fn a_server_never_ready_says_so_and_what_it_printed_last() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("temp dir");
    let (llama, _) = engine(
        dir.path(),
        "eprint 0.00.100.000 I srv    load_model: loading model '/models/qwen.gguf'\n\
         eprint 0.00.200.000 I load_tensors: loading model tensors, this can take a while...\n\
         sleep 60",
    );
    let spec = model(dir.path(), NEVER_READY_ID);
    let (_, core) = set_and_core(spec.clone(), &llama, 19_730);
    let mut request = prepare(
        spec.clone(),
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
    );
    request.health_deadline_secs = 8;

    let failure = super::launch::run(
        Arc::clone(&core),
        Arc::new(AdmissionQueue::new()),
        Arc::new(OneModel(spec)),
        Arc::new(NoopResidencyObserver),
        request,
    )
    .await
    .expect_err("a server never ready does not load")
    .to_string();

    core.write()
        .await
        .kill(NEVER_READY_ID, crate::process::StopReason::Asked)
        .await
        .ok();
    assert!(failure.contains("not ready within 8 s"), "{failure}");
    assert!(
        failure.contains("load_tensors: loading model tensors, this can take a while..."),
        "{failure}"
    );
}

/// A server whose binary is not there, or that the system would not start,
/// is said as its status says it, whether or not a status asked first: one
/// failure, one shape. Any other reason the core gives is its own; another
/// program on the port is said as such.
#[test]
fn a_server_not_started_reads_as_its_status_does() {
    use super::launch::launch_failure::{not_healthy, not_started};
    use gglib_core::utils::process::bounded::Failure;

    let dir = tempfile::tempdir().expect("temp dir");
    let missing = dir.path().join("llama-server");
    let status = crate::engine::validate_engine_at(RuntimeKind::Llama, &missing)
        .expect_err("not there")
        .to_string();
    let present = gglib_test_proc::install(dir.path(), "present-server", "");
    let refused = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        .context("Failed to spawn llama-server");

    assert_eq!(
        not_started(
            &anyhow::anyhow!("whatever the spawn said"),
            RuntimeKind::Llama,
            &missing
        ),
        LoadCause::EngineNotWorking {
            failure: Failure::NotFound,
            reason: status,
        }
    );
    assert!(matches!(
        not_started(&refused, RuntimeKind::Llama, &present),
        LoadCause::EngineNotWorking {
            failure: Failure::NotExecutable,
            ..
        }
    ));
    assert_eq!(
        not_started(
            &anyhow::anyhow!("Model 3 is already running"),
            RuntimeKind::Llama,
            &present
        ),
        LoadCause::NotStarted {
            reason: "Model 3 is already running".to_owned()
        }
    );
    let held: anyhow::Error = crate::process::health::PortHeld::for_test(9000).into();
    assert_eq!(
        not_healthy(&held, &crate::process::OutputTail::default()),
        Some(LoadCause::PortTaken { port: 9000 })
    );
}
