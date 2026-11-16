//! A model that cannot load says why (#1959 C5): an engine already found
//! not working is refused before anything is started, with the reason its
//! status gives; a server that stops while it loads, or is never ready, is
//! reported with what it printed and how it ended.
//!
//! Each engine is a `gglib-test-proc` stand-in, which runs its whole script
//! whatever it is asked, `--version` included.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::cache_config::CacheRamSetting;
use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec, NoopResidencyObserver};
use gglib_core::server_config::ServerConfigOptions;
use tokio::sync::RwLock;

use super::residency_tests::{OneModel, launch_spec};
use super::{ResidentSet, prepare};
use crate::process::RuntimeBinaries;
use crate::process::admission::AdmissionQueue;
use crate::process::core::GuiProcessCore;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const REFUSED_ID: u32 = 999_301;
const MENDED_ID: u32 = 999_302;
const STOPPED_ID: u32 = 999_303;
const NEVER_READY_ID: u32 = 999_304;

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
    lines
        .iter()
        .map(|line| format!("eprint {line}\n"))
        .collect()
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
    let (set, core) = set_and_core(model(dir.path(), REFUSED_ID), &llama, 19_530);

    let refusal = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("an engine known not to work is refused")
        .to_string();

    let ran = last_run(&argv);
    let tracked = core.read().await.list_all().len();
    core.write().await.kill(REFUSED_ID).await.ok();
    assert!(
        ran.is_empty() || ran == ["--version"],
        "only the check may run the engine, not a load: {ran:?}"
    );
    assert_eq!(tracked, 0, "no server is started");
    assert!(
        refusal.contains(&reason),
        "the refusal gives the status's reason\n  refusal: {refusal}\n  reason: {reason}"
    );
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
    let (set, core) = set_and_core(model(dir.path(), MENDED_ID), &llama, 19_540);

    let failure = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("the stand-in exits at once, so the load still fails")
        .to_string();

    let ran = last_run(&argv);
    core.write().await.kill(MENDED_ID).await.ok();
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
    let (set, core) = set_and_core(model(dir.path(), STOPPED_ID), &llama, 19_550);

    let failure = set
        .admit(&core, "qwen", None, None, LaunchOverrides::default())
        .await
        .expect_err("a server that exits does not load")
        .to_string();

    core.write().await.kill(STOPPED_ID).await.ok();
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
    let (_, core) = set_and_core(spec.clone(), &llama, 19_560);
    let mut request = prepare(
        spec.clone(),
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &LaunchOverrides::default(),
    );
    request.health_deadline_secs = 3;

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

    core.write().await.kill(NEVER_READY_ID).await.ok();
    assert!(failure.contains("not ready within 3 s"), "{failure}");
    assert!(
        failure.contains("load_tensors: loading model tensors, this can take a while..."),
        "{failure}"
    );
}
