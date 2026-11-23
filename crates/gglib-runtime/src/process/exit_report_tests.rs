//! What gglib's log says when a server it started stops (#2216): an exit
//! nobody asked for names its status and holds the last lines it printed,
//! at warn; a stop that was asked for is one line saying why, and never a
//! failure.
//!
//! Each server is a `gglib-test-proc` stand-in, the same on every OS.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::domain::{ImageFamily, RuntimeKind};
use gglib_core::ports::ServerConfig;
use tokio::sync::RwLock;
use tracing::dispatcher::DefaultGuard;

use super::{REPORTED_LINE_CHARS, REPORTED_LINES, StopReason, exit_said, is_a_stop, watch};
use crate::pidfile::delete_pidfile;
use crate::process::core::GuiProcessCore;
use crate::process::types::ServerInfo;
use crate::process::{RuntimeBinaries, SpawnConfig};
use crate::sd::SdServerConfig;

/// Ids no other test in this binary uses: `spawn` writes a pidfile keyed by
/// model id into this binary's data root.
const EXITS_ID: u32 = 999_701;
const IMAGE_EXITS_ID: u32 = 999_702;
const ASKED_ID: u32 = 999_703;
const EXITED_THEN_ASKED_ID: u32 = 999_704;
const SHUTDOWN_ID: u32 = 999_705;
const EXITS_CLEANLY_ID: u32 = 999_706;
const LAUNCHED_ID: u32 = 999_707;
const REAPED_ID: u32 = 999_708;

/// How long a test waits for a line it expects.
const LOG_WAIT: Duration = Duration::from_secs(10);

/// What every report of an exit by itself says; a line the server printed
/// is also in the log at debug, so a test waits for this.
const EXITED: &str = "exited by itself";

/// How long a test that expects nothing more waits for it, once the
/// server's output has ended: far past the watch's own look for a status,
/// and past the wait for its output to settle before a report is written.
const QUIET_WAIT: Duration = Duration::from_secs(3);

/// What a capture's writer has been given.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Everything logged on this thread while it lives. The tests run on
/// tokio's current-thread runtime, so the tasks that write the reports run
/// on this thread too.
struct Capture {
    captured: Captured,
    _default: DefaultGuard,
    _second: tracing::Dispatch,
}

impl Capture {
    fn start() -> Self {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        // Other tests hit the same callsites on other threads. A second
        // registered dispatcher makes a callsite one of them hit first
        // consult this capture too, instead of caching that nobody listens.
        let capture = tracing::Dispatch::new(subscriber);
        let second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        let default = tracing::dispatcher::set_default(&capture);
        tracing::callsite::rebuild_interest_cache();
        Self {
            captured,
            _default: default,
            _second: second,
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.captured.0.lock().unwrap()).into_owned()
    }

    /// The log once it holds `wanted`, or as it is after [`LOG_WAIT`].
    async fn once_it_says(&self, wanted: &str) -> String {
        let deadline = tokio::time::Instant::now() + LOG_WAIT;
        loop {
            let text = self.text();
            if text.contains(wanted) || tokio::time::Instant::now() >= deadline {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// A core whose llama-server and `sd-server` are stand-ins in `dir`
/// following `llama` and `sd`, and a model file for either.
fn core(dir: &Path, base_port: u16, llama: &str, sd: &str) -> Arc<RwLock<GuiProcessCore>> {
    gglib_core::paths::isolate_data_root();
    std::fs::write(dir.join("model.gguf"), b"not really a gguf").expect("write model file");
    let binaries = RuntimeBinaries {
        llama: gglib_test_proc::install(dir, "llama-server", llama),
        sd: gglib_test_proc::install(dir, "sd-server", sd),
    };
    Arc::new(RwLock::new(GuiProcessCore::new(base_port, binaries)))
}

/// A llama-server launch of `qwen` as model `id`.
fn qwen(dir: &Path, id: u32, base_port: u16) -> SpawnConfig {
    SpawnConfig::Llama(ServerConfig::new(
        i64::from(id),
        "qwen".to_owned(),
        dir.join("model.gguf"),
        base_port,
    ))
}

/// An `sd-server` launch of `flux` as model `id`.
fn flux(dir: &Path, id: u32) -> SpawnConfig {
    SpawnConfig::Sd(SdServerConfig {
        model_id: i64::from(id),
        model_name: "flux".to_owned(),
        model_path: dir.join("model.gguf"),
        family: ImageFamily::Flux1,
        components: Vec::new(),
        port: None,
    })
}

/// Spawn `config` as model `id`, watched as a launch watches it; its pid.
async fn spawn_watched(core: &Arc<RwLock<GuiProcessCore>>, config: SpawnConfig, id: u32) -> u32 {
    let mut core_w = core.write().await;
    let (_, pid) = core_w.spawn(config).expect("spawn");
    let output = core_w.output_of(id).expect("tracked");
    drop(core_w);
    watch(core, id, pid, output);
    pid
}

/// Stop model `id` if it is still tracked, and remove its pidfile.
async fn clean_up(core: &Arc<RwLock<GuiProcessCore>>, id: u32) {
    core.write().await.kill(id, StopReason::Asked).await.ok();
    delete_pidfile(i64::from(id)).ok();
}

/// The script a stand-in follows to print `lines`, on stdout and stderr in
/// turn, and then exit with `code`.
fn prints_then_exits(lines: &[&str], code: u8) -> String {
    let mut script = String::new();
    for (i, line) in lines.iter().enumerate() {
        script.push_str(if i % 2 == 0 { "print " } else { "eprint " });
        script.push_str(line);
        script.push('\n');
    }
    script.push_str("exit ");
    script.push_str(&code.to_string());
    script
}

const SAID: &[&str] = &[
    "loading the model",
    "could not allocate the KV cache",
    "giving up",
];

/// The report in `log` that starts with `head`, from its head to the end
/// of the log, after checking that its line is a warning.
fn report_in<'a>(log: &'a str, head: &str) -> &'a str {
    let at = log
        .find(head)
        .unwrap_or_else(|| panic!("no report starting {head:?} in:\n{log}"));
    let line_start = log[..at].rfind('\n').map_or(0, |i| i + 1);
    assert!(log[line_start..at].contains(" WARN "), "{log}");
    &log[at..]
}

/// The issue's first check: a llama-server that prints a few lines and
/// exits by itself with 3. The log names the status and holds those lines,
/// at warn, once; a stop asked of it afterwards adds nothing.
#[tokio::test]
async fn a_llama_server_that_exits_by_itself_leaves_its_status_and_last_lines() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_800, &prints_then_exits(SAID, 3), "sleep 30");

    let pid = spawn_watched(&core, qwen(dir.path(), EXITS_ID, 19_800), EXITS_ID).await;
    let log = capture.once_it_says(EXITED).await;
    clean_up(&core, EXITS_ID).await;
    // A second report would be written once the output settles, as the
    // first was.
    tokio::time::sleep(QUIET_WAIT).await;
    let after_stop = capture.text();

    let report = report_in(
        &log,
        &format!(
            "llama-server for qwen (model {EXITS_ID}, pid {pid}, port 19800) exited by \
             itself, with exit code 3. The last 3 lines it printed:"
        ),
    );
    // Each stream is read on its own, so the two interleave as they are read.
    for line in SAID {
        assert!(report.contains(&format!("\n  | {line}")), "{log}");
    }
    assert_eq!(
        after_stop.matches("llama-server for qwen").count(),
        1,
        "{after_stop}"
    );
}

/// The same for an `sd-server`: one reporter serves both runtimes.
#[tokio::test]
async fn an_sd_server_that_exits_by_itself_leaves_its_status_and_last_lines() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_810, "sleep 30", &prints_then_exits(SAID, 3));

    let pid = spawn_watched(&core, flux(dir.path(), IMAGE_EXITS_ID), IMAGE_EXITS_ID).await;
    let log = capture.once_it_says(EXITED).await;
    clean_up(&core, IMAGE_EXITS_ID).await;

    let report = report_in(
        &log,
        &format!("sd-server for flux (model {IMAGE_EXITS_ID}, pid {pid}, port "),
    );
    assert!(
        report.contains(") exited by itself, with exit code 3. The last 3 lines it printed:"),
        "{log}"
    );
    for line in SAID {
        assert!(report.contains(&format!("\n  | {line}")), "{log}");
    }
}

/// The issue's second check: a server stopped on request. The log says it
/// was asked to, in one line that is not a warning, and nothing says it
/// exited by itself, though its watch sees its output end.
#[tokio::test]
async fn a_server_stopped_on_request_is_logged_as_asked_to() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_820, "print ready\nsleep 30", "sleep 30");
    spawn_watched(&core, qwen(dir.path(), ASKED_ID, 19_820), ASKED_ID).await;
    let output = core.read().await.output_of(ASKED_ID).expect("tracked");
    capture.once_it_says("ready").await;

    core.write()
        .await
        .kill(ASKED_ID, StopReason::Asked)
        .await
        .expect("stop");
    let _ = tokio::time::timeout(LOG_WAIT, output.ended()).await;
    tokio::time::sleep(QUIET_WAIT).await;
    let log = capture.text();
    delete_pidfile(i64::from(ASKED_ID)).ok();

    let said: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("llama-server for qwen"))
        .collect();
    assert_eq!(said.len(), 1, "{log}");
    assert!(
        said[0].contains("llama-server for qwen stopped, as it was asked to"),
        "{log}"
    );
    assert!(said[0].contains(" INFO "), "{log}");
    assert!(!log.contains(" WARN "), "{log}");
}

/// A server that exited by itself before anything noticed, and is then
/// stopped (as a failed health check stops one): what the log says is that
/// it exited by itself, with its status, and not that it was asked to.
#[tokio::test]
async fn a_server_that_exited_before_its_stop_is_logged_as_exiting_by_itself() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let script = prints_then_exits(&["out of memory"], 1);
    let core = core(dir.path(), 19_830, &script, "sleep 30");
    // Not watched: nothing notices the exit until the stop.
    let (_, pid) = core
        .write()
        .await
        .spawn(qwen(dir.path(), EXITED_THEN_ASKED_ID, 19_830))
        .expect("spawn");
    let output = core
        .read()
        .await
        .output_of(EXITED_THEN_ASKED_ID)
        .expect("tracked");
    let _ = tokio::time::timeout(LOG_WAIT, output.ended()).await;
    for _ in 0..100 {
        if core.write().await.has_exited(EXITED_THEN_ASKED_ID) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    core.write()
        .await
        .kill(EXITED_THEN_ASKED_ID, StopReason::Unhealthy)
        .await
        .expect("stop");
    let log = capture.once_it_says(EXITED).await;
    delete_pidfile(i64::from(EXITED_THEN_ASKED_ID)).ok();

    let report = report_in(
        &log,
        &format!(
            "llama-server for qwen (model {EXITED_THEN_ASKED_ID}, pid {pid}, port 19830) \
             exited by itself, with exit code 1. The last line it printed:"
        ),
    );
    assert!(report.contains("\n  | out of memory"), "{log}");
    assert!(!log.contains("stopped, as"), "{log}");
}

/// gglib shutting down kills every server at once: each is logged as
/// stopped because gglib is shutting down, and neither its watch nor the
/// stop that follows reports the kill as an exit by itself.
#[tokio::test]
async fn a_shutdown_is_not_logged_as_a_failure() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_840, "sleep 30", "sleep 30");
    spawn_watched(&core, qwen(dir.path(), SHUTDOWN_ID, 19_840), SHUTDOWN_ID).await;
    let output = core.read().await.output_of(SHUTDOWN_ID).expect("tracked");

    core.write().await.kill_all_now();
    let _ = tokio::time::timeout(LOG_WAIT, output.ended()).await;
    tokio::time::sleep(QUIET_WAIT).await;
    clean_up(&core, SHUTDOWN_ID).await;
    let log = capture.text();

    let said: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("llama-server for qwen"))
        .collect();
    assert_eq!(said.len(), 1, "{log}");
    assert!(
        said[0].contains("llama-server for qwen stopped, as gglib is shutting down"),
        "{log}"
    );
    assert!(!log.contains(" WARN "), "{log}");
}

/// A server that exits by itself with 0, as llama-server does once it has
/// handled a stop signal gglib did not send: the report names the status
/// and holds its lines, at info and not warn; a stop afterwards adds
/// nothing.
#[tokio::test]
async fn a_server_that_exits_with_0_is_reported_at_info_with_its_lines() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_850, "print bye\nexit 0", "sleep 30");

    let pid = spawn_watched(
        &core,
        qwen(dir.path(), EXITS_CLEANLY_ID, 19_850),
        EXITS_CLEANLY_ID,
    )
    .await;
    capture.once_it_says(EXITED).await;
    clean_up(&core, EXITS_CLEANLY_ID).await;
    tokio::time::sleep(QUIET_WAIT).await;
    let log = capture.text();

    let head = format!(
        "llama-server for qwen (model {EXITS_CLEANLY_ID}, pid {pid}, port 19850) exited by \
         itself, with exit code 0. The last line it printed:\n  | bye"
    );
    let at = log
        .find(&head)
        .unwrap_or_else(|| panic!("no report starting {head:?} in:\n{log}"));
    let line_start = log[..at].rfind('\n').map_or(0, |i| i + 1);
    assert!(log[line_start..at].contains(" INFO "), "{log}");
    assert_eq!(log.matches("llama-server for qwen").count(), 1, "{log}");
    assert!(!log.contains(" WARN "), "{log}");
}

/// A 0, a Ctrl-C, a closed terminal and a request to stop are stops; any
/// other code or signal, a kill or a crash among them, is a failure.
#[test]
fn a_zero_or_a_stop_signal_is_a_stop_and_anything_else_a_failure() {
    for stop in [
        (Some(0), None),
        (None, Some(1)),
        (None, Some(2)),
        (None, Some(15)),
    ] {
        assert!(is_a_stop(stop.0, stop.1), "{stop:?}");
    }
    for failure in [
        (Some(1), None),
        (Some(3), None),
        (Some(-1_073_741_819), None),
        (None, Some(9)),
        (None, Some(6)),
        (None, Some(11)),
        (None, None),
    ] {
        assert!(!is_a_stop(failure.0, failure.1), "{failure:?}");
    }
}

/// A server nothing watches, found dead by `cleanup_dead`: the sweep that
/// takes it out of tracking reports how it ended and what it printed, at
/// warn, once; a second sweep and a stop asked of it afterwards add
/// nothing.
#[tokio::test]
async fn a_server_cleanup_dead_takes_is_reported_once() {
    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let core = core(dir.path(), 19_870, &prints_then_exits(SAID, 4), "sleep 30");
    // Not watched: only the sweep can tell of it.
    let (_, pid) = core
        .write()
        .await
        .spawn(qwen(dir.path(), REAPED_ID, 19_870))
        .expect("spawn");
    let output = core.read().await.output_of(REAPED_ID).expect("tracked");
    let _ = tokio::time::timeout(LOG_WAIT, output.ended()).await;
    let mut swept = Vec::new();
    for _ in 0..100 {
        swept = core.write().await.cleanup_dead();
        if !swept.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(swept, [REAPED_ID], "the sweep found it dead");

    let log = capture.once_it_says(EXITED).await;
    assert!(core.write().await.cleanup_dead().is_empty());
    assert!(
        core.write()
            .await
            .kill(REAPED_ID, StopReason::Asked)
            .await
            .is_err(),
        "the sweep stopped tracking it"
    );
    tokio::time::sleep(QUIET_WAIT).await;
    let after = capture.text();
    delete_pidfile(i64::from(REAPED_ID)).ok();

    let report = report_in(
        &log,
        &format!(
            "llama-server for qwen (model {REAPED_ID}, pid {pid}, port 19870) exited by \
             itself, with exit code 4. The last 3 lines it printed:"
        ),
    );
    for line in SAID {
        assert!(report.contains(&format!("\n  | {line}")), "{log}");
    }
    assert_eq!(after.matches("llama-server for qwen").count(), 1, "{after}");
    assert!(!after.contains("stopped, as"), "{after}");
}

/// The issue's case through a launch: an image model launched, healthy and
/// resident, whose `sd-server` then exits by itself. The watch the launch
/// started reports it, with nothing asking after it.
#[tokio::test]
async fn a_launched_server_that_exits_after_it_serves_is_reported() {
    use crate::process::residency::ResidentSet;
    use crate::process::residency::residency_tests::{OneModel, launch_spec};
    use crate::sd::fake_server::FakeSdServer;
    use gglib_core::cache_config::CacheRamSetting;
    use gglib_core::domain::{ComponentRole, ModelComponent};
    use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec};
    use gglib_core::server_config::ServerConfigOptions;

    let capture = Capture::start();
    let dir = tempfile::tempdir().expect("temp dir");
    let argv = dir.path().join("sd-server.argv");
    // Long enough to be found healthy first, by the fake below.
    let script = format!(
        "argv {}\nsleep 5\neprint ggml: out of device memory\nexit 3",
        argv.display()
    );
    let core = core(dir.path(), 19_860, "sleep 30", &script);
    let file = |name: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").expect("write");
        path
    };
    let component = |role, name: &str| ModelComponent {
        role,
        path: file(name),
    };
    let spec = ModelLaunchSpec {
        name: "flux".to_owned(),
        file_path: file("flux.gguf"),
        image_family: Some(ImageFamily::Flux1),
        components: vec![
            component(ComponentRole::T5xxl, "t5xxl.safetensors"),
            component(ComponentRole::Vae, "ae.safetensors"),
            component(ComponentRole::ClipL, "clip_l.safetensors"),
        ],
        ..launch_spec(LAUNCHED_ID, "flux")
    };
    let set = Arc::new(ResidentSet::unobserved(
        Arc::new(OneModel(spec)),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    ));
    let admitting = {
        let (set, core) = (Arc::clone(&set), Arc::clone(&core));
        tokio::spawn(async move {
            set.admit(&core, "flux", None, None, LaunchOverrides::default())
                .await
                .map(|admission| admission.target.port)
        })
    };
    let port = tokio::time::timeout(LOG_WAIT, async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&argv)
                && let Some(at) = text.lines().position(|arg| arg == "--listen-port")
            {
                return text
                    .lines()
                    .nth(at + 1)
                    .expect("a port")
                    .parse::<u16>()
                    .expect("a port");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the stand-in sd-server was started");
    let _fake = FakeSdServer::serve_on(port).await;
    let admitted = admitting.await.expect("the launch ran");

    let log = capture.once_it_says(EXITED).await;
    set.recycle(1, &core, StopReason::Asked).await;
    delete_pidfile(i64::from(LAUNCHED_ID)).ok();

    assert_eq!(
        admitted.ok(),
        Some(port),
        "it was healthy and admitted first"
    );
    let report = report_in(
        &log,
        &format!("sd-server for flux (model {LAUNCHED_ID}, pid "),
    );
    assert!(
        report.contains(&format!(
            ", port {port}) exited by itself, with exit code 3. The last line it printed:\n  | ggml: out of device memory"
        )),
        "{log}"
    );
    assert!(!log.contains("stopped, as"), "{log}");
}

/// The status a stand-in in `dir` exits with when its script is `exit 2`.
fn status_of_exit_2(dir: &Path) -> std::process::ExitStatus {
    let program: PathBuf = gglib_test_proc::install(dir, "exits", "exit 2");
    gglib_core::utils::process::cmd(program)
        .status()
        .expect("run")
}

/// The report is bounded: the last [`REPORTED_LINES`] lines, each cut to
/// [`REPORTED_LINE_CHARS`] characters.
#[test]
fn the_report_holds_only_the_last_lines_each_cut() {
    let dir = tempfile::tempdir().expect("temp dir");
    let status = status_of_exit_2(dir.path());
    let info = ServerInfo::new(1, "qwen".to_owned(), 7, 9000, 0, RuntimeKind::Llama);
    let mut lines: Vec<String> = (1..=50).map(|n| format!("line {n}")).collect();
    lines.push("x".repeat(REPORTED_LINE_CHARS + 10));

    let said = exit_said(&info, status, &lines);

    let mut reported = said.split("\n  | ");
    let head = reported.next().expect("a head");
    let reported: Vec<&str> = reported.collect();
    assert_eq!(
        head,
        format!(
            "llama-server for qwen (model 1, pid 7, port 9000) exited by itself, with \
             exit code 2. The last {REPORTED_LINES} lines it printed:"
        )
    );
    let first_kept = lines.len() - REPORTED_LINES;
    assert_eq!(reported.len(), REPORTED_LINES);
    assert_eq!(reported[0], lines[first_kept]);
    assert_eq!(
        reported[REPORTED_LINES - 1],
        "x".repeat(REPORTED_LINE_CHARS)
    );
}

/// A server that printed nothing is said to have printed nothing.
#[test]
fn a_server_that_printed_nothing_is_said_to_have() {
    let dir = tempfile::tempdir().expect("temp dir");
    let status = status_of_exit_2(dir.path());
    let info = ServerInfo::new(
        1,
        "flux".to_owned(),
        7,
        9000,
        0,
        RuntimeKind::StableDiffusion,
    );

    assert_eq!(
        exit_said(&info, status, &[]),
        "sd-server for flux (model 1, pid 7, port 9000) exited by itself, with exit code 2; \
         it printed nothing"
    );
}
