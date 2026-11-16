//! An install of the Visual C++ runtime with its download, its check and
//! its installer faked.
//!
//! The installer stand-in is `gglib-test-proc`, which exits with a byte: a
//! Unix exit status is 8 bits, so 3010, 1638 and 1602 cannot be exited on
//! macOS and Linux. Their meaning is [`exit_outcome`]'s, tested as values,
//! and their words as [`report`]'s; the stand-in runs the installs that exit
//! 0 or 1 and the one that never exits, on every OS. No test downloads or
//! runs the real installer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::domain::RuntimeKind;
use gglib_core::host::HostOs;
use tokio::fs::File;

use super::{
    Digest, INSTALLER_STEM, Installer, Installing, RuntimeInstall, RuntimeState, after_exit,
    detached, digest_of, exit_outcome, fresh_folder, production, report,
};

/// Each exit code the installer documents.
#[test]
fn each_exit_code_means_what_microsoft_says() {
    let table = [
        (Some(0), RuntimeInstall::Installed),
        (Some(3010), RuntimeInstall::RestartNeeded),
        (Some(1641), RuntimeInstall::RestartNeeded),
        (Some(1638), RuntimeInstall::NewerInstalled),
        (Some(1602), RuntimeInstall::Declined),
        (Some(1223), RuntimeInstall::Declined),
        (Some(740), RuntimeInstall::Refused),
        (Some(5), RuntimeInstall::Refused),
        (Some(1618), RuntimeInstall::Busy),
        (Some(1), RuntimeInstall::Failed),
        (None, RuntimeInstall::Failed),
    ];
    for (code, outcome) in table {
        assert_eq!(exit_outcome(code), outcome, "{code:?}");
    }
}

/// What each outcome says, with the engine checked again where the install
/// may have changed something.
#[test]
fn each_outcome_says_what_happened() {
    let says = |outcome, detail: Option<&str>, works| {
        report(RuntimeKind::Llama, outcome, detail, works).says
    };
    assert_eq!(
        says(RuntimeInstall::Installed, Some("0"), Some(true)),
        "Installed the Microsoft Visual C++ Redistributable (x64). llama-server works now."
    );
    assert_eq!(
        says(RuntimeInstall::RestartNeeded, Some("3010"), Some(false)),
        "Installed the Microsoft Visual C++ Redistributable (x64); Windows needs a restart to \
         finish it. llama-server still cannot start: restart Windows, then try again."
    );
    assert_eq!(
        says(RuntimeInstall::NewerInstalled, Some("1638"), Some(false)),
        "A newer Microsoft Visual C++ Redistributable (x64) is already installed. llama-server \
         still cannot start, so a file that came with llama.cpp may be missing: reinstall \
         llama.cpp."
    );
    assert_eq!(
        says(RuntimeInstall::Damaged, Some("1638"), None),
        "Windows says a newer Microsoft Visual C++ Redistributable (x64) is already installed, \
         but its files are missing, so it is damaged and installing it again does not help. \
         Repair it in Windows: Settings › Apps › Installed apps › Microsoft Visual C++ v14 \
         Redistributable (x64) (2015-2022 on an older one) › Modify › Repair, then check again."
    );
    assert_eq!(
        says(RuntimeInstall::Declined, Some("1602"), None),
        "Nothing was installed: Windows' permission was not given."
    );
    assert_eq!(
        says(RuntimeInstall::Refused, Some("740"), None),
        "Windows refused the Microsoft Visual C++ Redistributable (x64) installer the \
         permission it needs (740). Nothing was installed."
    );
    assert_eq!(
        says(RuntimeInstall::Busy, Some("1618"), None),
        "Windows is installing something else, so the Microsoft Visual C++ Redistributable \
         (x64) could not be installed now. Wait for that to finish, then try again."
    );
    assert_eq!(
        says(RuntimeInstall::TimedOut, None, None),
        "gglib stopped waiting for the Microsoft Visual C++ Redistributable (x64) installer \
         after 10 minutes. Windows' question for permission was probably waiting behind a \
         flashing shield on the taskbar; the installer may still finish if it is answered. \
         Check again in a while, or install it by hand from \
         https://aka.ms/vc14/vc_redist.x64.exe."
    );
    assert_eq!(
        says(RuntimeInstall::Failed, Some("2"), None),
        "The Microsoft Visual C++ Redistributable (x64) installer did not finish (2). It \
         can be installed by hand from https://aka.ms/vc14/vc_redist.x64.exe."
    );
    assert_eq!(
        says(RuntimeInstall::NotNeeded, None, None),
        "Nothing to install: the Microsoft Visual C++ Redistributable (x64) is for Windows only."
    );
    let works = report(
        RuntimeKind::Llama,
        RuntimeInstall::NewerInstalled,
        None,
        Some(true),
    );
    assert!(works.works);
    let unchecked = report(RuntimeKind::Llama, RuntimeInstall::Declined, None, None);
    assert!(!unchecked.works);
}

/// One install at a time per lock.
#[test]
fn a_second_install_waits_for_the_first() {
    static LOCK: AtomicBool = AtomicBool::new(false);
    let first = Installing::take(&LOCK).expect("nothing installs yet");
    assert!(Installing::take(&LOCK).is_none());
    drop(first);
    assert!(Installing::take(&LOCK).is_some());
}

/// Each install's folder is a new one, empty, with a name of its own, and
/// goes when it is let go.
#[test]
fn each_install_has_a_new_folder_of_its_own() {
    let first = fresh_folder().unwrap();
    let second = fresh_folder().unwrap();
    assert_ne!(first.path(), second.path());
    for folder in [&first, &second] {
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
        let name = folder.path().file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("gglib-visual-cpp-runtime-"), "{name}");
    }
    let path = first.path().to_path_buf();
    drop(first);
    assert!(!path.exists());
}

/// The install gglib runs checks the download with the real check: a file
/// nobody signed is refused, on every OS (off Windows, because there is no
/// Windows to ask).
#[test]
fn the_install_gglib_runs_checks_the_download_for_real() {
    let dir = tempfile::tempdir().unwrap();
    let unsigned = dir.path().join("vc_redist.x64.exe");
    std::fs::write(&unsigned, b"not signed by anyone").unwrap();

    let installer = production(
        RuntimeKind::Llama,
        HostOs::Windows,
        dir.path().to_path_buf(),
    );

    assert!((installer.verify)(&unsigned).is_err());
    assert_eq!(installer.os, HostOs::Windows);
}

/// Whether a step of a fake install ran.
#[derive(Clone, Default)]
struct Ran(Arc<AtomicBool>);

impl Ran {
    fn set(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    fn get(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// How a fake install goes.
struct Fake {
    os: HostOs,
    verified: Result<(), String>,
    wait: Duration,
    works: bool,
    runtime: RuntimeState,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            os: HostOs::Windows,
            verified: Ok(()),
            wait: Duration::from_secs(30),
            works: true,
            runtime: RuntimeState::Installed,
        }
    }
}

impl Fake {
    /// The install in `dir`, whose check of the engine afterwards marks
    /// `rechecked`.
    fn installer(
        self,
        dir: &Path,
        rechecked: &Ran,
    ) -> Installer<
        impl FnOnce(&Path) -> Result<(), String> + Send + 'static + use<>,
        impl FnOnce() -> bool + Send + 'static + use<>,
        impl FnOnce() -> RuntimeState + Send + 'static + use<>,
    > {
        let rechecked = rechecked.clone();
        let Self {
            os,
            verified,
            wait,
            works,
            runtime,
        } = self;
        Installer {
            os,
            dir: dir.to_path_buf(),
            verify: move |_: &Path| verified,
            wait,
            recheck: move || {
                rechecked.set();
                works
            },
            runtime: move || runtime,
        }
    }
}

/// The SHA-256 of the file at `path`.
fn digest_at(path: &Path) -> Digest {
    digest_of(&std::fs::File::open(path).unwrap()).unwrap()
}

/// A download that puts the stand-in where the installer goes, following
/// `instructions`, marks `downloaded`, and says the digest of what it put
/// there. The stand-in is copied over the file the install created for it.
fn download(
    instructions: String,
    downloaded: &Ran,
) -> impl FnOnce(PathBuf, File) -> std::future::Ready<Result<Digest, String>> + Send + 'static {
    let downloaded = downloaded.clone();
    move |path: PathBuf, file: File| {
        downloaded.set();
        drop(file);
        let program =
            gglib_test_proc::install(path.parent().unwrap(), INSTALLER_STEM, &instructions);
        assert_eq!(program, path, "the stand-in is where the installer goes");
        std::future::ready(Ok(digest_at(&path)))
    }
}

/// A file changed after it was downloaded (here, its digest says other
/// bytes than it holds) is never checked or run: what is held must be what
/// was written.
#[tokio::test]
async fn a_file_changed_after_its_download_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv");
    let rechecked = Ran::default();
    let instructions = format!("argv {}\nexit 0", argv.display());
    let swapped = move |path: PathBuf, file: File| {
        drop(file);
        gglib_test_proc::install(path.parent().unwrap(), INSTALLER_STEM, &instructions);
        std::future::ready(Ok::<Digest, String>([7; 32]))
    };

    // The fake check passes everything: only the digest can refuse it.
    let report = Fake::default()
        .installer(dir.path(), &rechecked)
        .run(RuntimeKind::Llama, swapped)
        .await;

    assert_eq!(report.outcome, RuntimeInstall::NotVerified);
    assert!(
        report.says.contains("changed after it was downloaded"),
        "{}",
        report.says
    );
    assert!(!argv.exists(), "the installer ran");
    assert!(!rechecked.get());
}

/// An installer that exits 0 is run with its quiet switches, and the engine
/// is checked again.
#[tokio::test]
async fn an_install_that_exits_0_runs_quietly_and_checks_the_engine_again() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv");
    let (downloaded, rechecked) = (Ran::default(), Ran::default());

    let report = Fake::default()
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::Llama,
            download(format!("argv {}\nexit 0", argv.display()), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::Installed);
    assert!(report.works);
    assert!(rechecked.get());
    assert_eq!(
        std::fs::read_to_string(&argv)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["/install", "/quiet", "/norestart"]
    );
}

/// An install after which the engine still does not start says so.
#[tokio::test]
async fn an_install_after_which_the_engine_still_fails_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = Fake {
        works: false,
        ..Fake::default()
    };

    let report = fake
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::StableDiffusion,
            download("exit 0".to_owned(), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::Installed);
    assert!(!report.works);
    assert!(rechecked.get());
    assert!(
        report.says.contains("sd-server still cannot start"),
        "{}",
        report.says
    );
}

/// An installer that exits with a code it does not document failed, and
/// nothing is checked again.
#[tokio::test]
async fn an_install_that_exits_otherwise_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());

    let report = Fake::default()
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::Llama,
            download("exit 1".to_owned(), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::Failed);
    assert!(!report.works);
    assert!(!rechecked.get());
    assert!(report.says.contains("(1)"), "{}", report.says);
}

/// An installer that never exits is no longer waited for at the limit, and
/// the words do not claim it stopped.
#[tokio::test]
async fn an_installer_that_never_exits_is_no_longer_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = Fake {
        wait: Duration::from_secs(1),
        ..Fake::default()
    };

    let report = fake
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::Llama,
            download("sleep 60".to_owned(), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::TimedOut);
    assert!(
        report.says.contains("the installer may still finish"),
        "{}",
        report.says
    );
    assert!(!rechecked.get());
}

/// A download that fails its check is never run.
#[tokio::test]
async fn a_download_that_fails_its_check_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv");
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = Fake {
        verified: Err("its signer is CN=Someone, not Microsoft".to_owned()),
        ..Fake::default()
    };

    let report = fake
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::Llama,
            download(format!("argv {}\nexit 0", argv.display()), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::NotVerified);
    assert!(downloaded.get());
    assert!(!argv.exists(), "the installer ran");
    assert!(!rechecked.get());
    assert_eq!(
        report.says,
        "The download was not run: its signer is CN=Someone, not Microsoft. Nothing was \
         installed. It can be installed by hand from https://aka.ms/vc14/vc_redist.x64.exe."
    );
}

/// A download that fails says so, and nothing is checked or run.
#[tokio::test]
async fn a_download_that_fails_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let rechecked = Ran::default();
    let fake = Fake {
        verified: Err("not to be checked".to_owned()),
        ..Fake::default()
    };

    let report = fake
        .installer(dir.path(), &rechecked)
        .run(RuntimeKind::Llama, |_: PathBuf, _: File| {
            std::future::ready(Err("error sending request".to_owned()))
        })
        .await;

    assert_eq!(report.outcome, RuntimeInstall::NotDownloaded);
    assert!(!rechecked.get());
    assert_eq!(
        report.says,
        "Could not download the Microsoft Visual C++ Redistributable (x64) from Microsoft \
         (error sending request). Nothing was installed."
    );
}

/// A file already where the installer goes is refused, not reused or
/// written over: nothing is downloaded into it, checked or run.
#[tokio::test]
async fn a_file_already_where_the_installer_goes_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let planted = dir
        .path()
        .join(format!("{INSTALLER_STEM}{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&planted, b"planted").unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());

    let report = Fake::default()
        .installer(dir.path(), &rechecked)
        .run(
            RuntimeKind::Llama,
            download("exit 0".to_owned(), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::NotDownloaded);
    assert!(!downloaded.get());
    assert!(!rechecked.get());
    assert_eq!(std::fs::read(&planted).unwrap(), b"planted");
}

/// After 1638 the runtime is looked for again: with its DLLs there, the
/// engine is checked again; with them still missing it is damaged, said
/// with how to repair it, and nothing is checked again.
#[test]
fn after_1638_the_runtime_is_looked_for_again() {
    for (runtime, outcome, rechecks) in [
        (
            RuntimeState::Installed,
            RuntimeInstall::NewerInstalled,
            true,
        ),
        (RuntimeState::Damaged, RuntimeInstall::Damaged, false),
        (RuntimeState::Missing, RuntimeInstall::Damaged, false),
    ] {
        let rechecked = Ran::default();
        let marks = rechecked.clone();
        let report = after_exit(
            RuntimeKind::Llama,
            Ok(Some(1638)),
            move || runtime,
            move || {
                marks.set();
                true
            },
        );
        assert_eq!(report.outcome, outcome, "{runtime:?}");
        assert_eq!(rechecked.get(), rechecks, "{runtime:?}");
    }
}

/// An installer Windows would not start for want of rights is said to be
/// refused, as one that exits with that code is.
#[test]
fn an_installer_windows_refuses_to_start_is_refused() {
    let report = after_exit(
        RuntimeKind::Llama,
        Err(std::io::Error::from_raw_os_error(740)),
        || RuntimeState::Installed,
        || panic!("nothing was installed to check"),
    );
    assert_eq!(report.outcome, RuntimeInstall::Refused);
}

/// Off Windows nothing is downloaded, checked or run.
#[tokio::test]
async fn off_windows_there_is_nothing_to_install() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    for os in [HostOs::MacOs, HostOs::Linux] {
        let fake = Fake {
            os,
            ..Fake::default()
        };
        let report = fake
            .installer(dir.path(), &rechecked)
            .run(
                RuntimeKind::Llama,
                download("exit 0".to_owned(), &downloaded),
            )
            .await;
        assert_eq!(report.outcome, RuntimeInstall::NotNeeded, "{os:?}");
    }
    assert!(!downloaded.get());
    assert!(!rechecked.get());
}

/// Wait up to 30 s for `done`.
async fn until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..600 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{what} did not happen within 30 s");
}

/// A caller that stops waiting (a page reloaded during a repair) does not
/// stop the install: a second one is refused while it runs, and its folder
/// is removed, and the lock let go, when it ends.
#[tokio::test]
async fn an_install_outlives_a_caller_that_stops_waiting() {
    static LOCK: AtomicBool = AtomicBool::new(false);
    let marks = tempfile::tempdir().unwrap();
    let argv = marks.path().join("argv");
    let folder: Arc<Mutex<Option<PathBuf>>> = Arc::default();

    let seen = Arc::clone(&folder);
    let instructions = format!("argv {}\nsleep 2", argv.display());
    let caller = tokio::spawn(detached(
        &LOCK,
        RuntimeKind::Llama,
        move |dir: PathBuf| {
            *seen.lock().unwrap() = Some(dir.clone());
            Fake::default().installer(&dir, &Ran::default())
        },
        download(instructions, &Ran::default()),
    ));
    until("the installer starting", || argv.exists()).await;
    caller.abort();
    let _ = caller.await;

    let second = detached(
        &LOCK,
        RuntimeKind::Llama,
        |dir: PathBuf| Fake::default().installer(&dir, &Ran::default()),
        download("exit 0".to_owned(), &Ran::default()),
    )
    .await;
    assert_eq!(second.outcome, RuntimeInstall::AlreadyRunning);

    let dir = folder
        .lock()
        .unwrap()
        .clone()
        .expect("the install had a folder");
    until("the first install's folder going", || !dir.exists()).await;
    until("the lock being let go", || !LOCK.load(Ordering::SeqCst)).await;
}

/// What glibc's loader writes for a library that is not there, which the
/// engine check reads as an engine that cannot start.
const LOADER_LINE: &str = "./llama-server: error while loading shared libraries: \
                           libvulkan.so.1: cannot open shared object file: No such file or directory";

/// After a repair that made the engine work, the engine check has
/// forgotten that it could not start, so a load straight after it starts
/// the engine without being refused, and without asking it again: here the
/// engine is made slow to answer after the repair, and the load's check
/// still answers at once.
#[tokio::test]
async fn a_load_straight_after_a_repair_starts_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let engines = tempfile::tempdir().unwrap();
    let engine = gglib_test_proc::install(
        engines.path(),
        "llama-server",
        &format!("eprint {LOADER_LINE}\nexit 127"),
    );
    let kind = RuntimeKind::Llama;
    crate::engine::validate_engine_at(kind, &engine).expect_err("it cannot start");
    assert!(crate::engine::refuse_if_known_not_working(kind, &engine).is_err());

    // The installer puts the missing library in place: the engine starts.
    let mended = engine.clone();
    let instructions = gglib_test_proc::instructions_path(&engine);
    let base = Fake::default().installer(dir.path(), &Ran::default());
    let repaired = Installer {
        os: base.os,
        dir: base.dir,
        verify: base.verify,
        wait: base.wait,
        recheck: move || crate::engine::validate_engine_at(kind, &mended).is_ok(),
        runtime: base.runtime,
    };
    let installer_download = download("exit 0".to_owned(), &Ran::default());
    let report = repaired
        .run(kind, move |path: PathBuf, file: File| {
            std::fs::write(&instructions, "print version: 1 (abc)\nexit 0\n").unwrap();
            installer_download(path, file)
        })
        .await;
    assert_eq!(report.outcome, RuntimeInstall::Installed);
    assert!(report.works, "{}", report.says);

    std::fs::write(gglib_test_proc::instructions_path(&engine), "sleep 60\n").unwrap();
    let asked = std::time::Instant::now();
    assert!(crate::engine::refuse_if_known_not_working(kind, &engine).is_ok());
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "the load asked the engine again: {:?}",
        asked.elapsed()
    );
}
