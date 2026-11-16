//! Whether the Visual C++ runtime is there, and an install of it with its
//! download, its check and its installer faked.
//!
//! The installer stand-in is `gglib-test-proc`, which exits with a byte: a
//! Unix exit status is 8 bits, so 3010, 1638 and 1602 cannot be exited on
//! macOS and Linux. Their meaning is [`exit_outcome`]'s, tested as values,
//! and their words as [`report`]'s; the stand-in runs the installs that exit
//! 0 or 1 and the one that never exits, on every OS.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gglib_core::domain::RuntimeKind;

use super::{
    INSTALLER_STEM, Installer, Installing, RuntimeInstall, Signature, exit_outcome,
    microsoft_signed, parse_signature, report, runtime_dlls_in,
};

/// The runtime is there when all three of its DLLs are.
#[test]
fn the_runtime_is_there_when_its_dlls_are() {
    let system = tempfile::tempdir().unwrap();
    for dll in ["vcruntime140.dll", "msvcp140.dll"] {
        std::fs::write(system.path().join(dll), "").unwrap();
    }
    assert!(!runtime_dlls_in(system.path()), "one is missing");
    std::fs::write(system.path().join("vcruntime140_1.dll"), "").unwrap();
    assert!(runtime_dlls_in(system.path()));
}

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
        (Some(1618), RuntimeInstall::Failed),
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
        says(RuntimeInstall::Declined, Some("1602"), None),
        "Nothing was installed: Windows' permission was not given."
    );
    assert_eq!(
        says(RuntimeInstall::Failed, Some("1618"), None),
        "The Microsoft Visual C++ Redistributable (x64) installer did not finish (1618). It \
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

/// A signature is Microsoft's only when Windows says it is valid and its
/// certificate names Microsoft as the organisation.
#[test]
fn only_a_valid_signature_of_microsofts_is_run() {
    let microsoft =
        "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US";
    let signature = |status: &str, subject: Option<&str>| Signature {
        status: status.to_owned(),
        subject: subject.map(str::to_owned),
    };
    assert_eq!(
        microsoft_signed(&signature("Valid", Some(microsoft))),
        Ok(())
    );
    for (status, subject) in [
        ("NotSigned", None),
        ("HashMismatch", Some(microsoft)),
        ("NotTrusted", Some(microsoft)),
        ("Valid", None),
        (
            "Valid",
            Some("CN=Someone, O=Not Microsoft Corporation, C=US"),
        ),
        (
            "Valid",
            Some("CN=Microsoft Corporation, O=Microsoft Corporation Ltd"),
        ),
    ] {
        assert!(
            microsoft_signed(&signature(status, subject)).is_err(),
            "{status} {subject:?}"
        );
    }
}

/// What the check prints is read as its status, then its subject.
#[test]
fn the_check_is_read_as_a_status_and_a_subject() {
    assert_eq!(
        parse_signature("Valid\r\nCN=Microsoft Corporation, O=Microsoft Corporation\r\n"),
        Signature {
            status: "Valid".to_owned(),
            subject: Some("CN=Microsoft Corporation, O=Microsoft Corporation".to_owned()),
        }
    );
    assert_eq!(
        parse_signature("NotSigned\n"),
        Signature {
            status: "NotSigned".to_owned(),
            subject: None,
        }
    );
}

/// One install at a time in a process.
#[test]
fn a_second_install_waits_for_the_first() {
    let first = Installing::take().expect("nothing installs yet");
    assert!(Installing::take().is_none());
    drop(first);
    assert!(Installing::take().is_some());
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

/// A fake install in `dir`: on `os`, with a check that answers `verified`,
/// an installer of `wait`, and a check of the engine afterwards that answers
/// `works` and marks `rechecked`.
fn installer(
    os: &'static str,
    dir: &Path,
    verified: Result<(), String>,
    wait: Duration,
    works: bool,
    rechecked: &Ran,
) -> Installer<
    impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
    impl FnOnce() -> bool + Send + 'static,
> {
    let rechecked = rechecked.clone();
    Installer {
        os,
        dir: dir.to_path_buf(),
        verify: move |_: &Path| verified,
        wait,
        recheck: move || {
            rechecked.set();
            works
        },
    }
}

/// A download that puts the stand-in where the installer goes, following
/// `instructions`, and marks `downloaded`.
fn download(
    instructions: String,
    downloaded: &Ran,
) -> impl FnOnce(PathBuf) -> std::future::Ready<Result<(), String>> {
    let downloaded = downloaded.clone();
    move |path: PathBuf| {
        downloaded.set();
        let program =
            gglib_test_proc::install(path.parent().unwrap(), INSTALLER_STEM, &instructions);
        assert_eq!(program, path, "the stand-in is where the installer goes");
        std::future::ready(Ok(()))
    }
}

/// An installer that exits 0 is run with its quiet switches, and the engine
/// is checked again.
#[tokio::test]
async fn an_install_that_exits_0_runs_quietly_and_checks_the_engine_again() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv");
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = installer(
        "windows",
        dir.path(),
        Ok(()),
        Duration::from_secs(30),
        true,
        &rechecked,
    );

    let report = fake
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
    let fake = installer(
        "windows",
        dir.path(),
        Ok(()),
        Duration::from_secs(30),
        false,
        &rechecked,
    );

    let report = fake
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
    let fake = installer(
        "windows",
        dir.path(),
        Ok(()),
        Duration::from_secs(30),
        true,
        &rechecked,
    );

    let report = fake
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

/// An installer that never exits is stopped at its wait.
#[tokio::test]
async fn an_installer_that_never_exits_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = installer(
        "windows",
        dir.path(),
        Ok(()),
        Duration::from_secs(1),
        true,
        &rechecked,
    );

    let report = fake
        .run(
            RuntimeKind::Llama,
            download("sleep 60".to_owned(), &downloaded),
        )
        .await;

    assert_eq!(report.outcome, RuntimeInstall::TimedOut);
    assert!(!rechecked.get());
}

/// A download that fails its check is never run.
#[tokio::test]
async fn a_download_that_fails_its_check_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv");
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    let fake = installer(
        "windows",
        dir.path(),
        Err("it is signed by CN=Someone, not by Microsoft".to_owned()),
        Duration::from_secs(30),
        true,
        &rechecked,
    );

    let report = fake
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
        "The download was not run: it is signed by CN=Someone, not by Microsoft. Nothing was \
         installed. It can be installed by hand from https://aka.ms/vc14/vc_redist.x64.exe."
    );
}

/// A download that fails says so, and nothing is checked or run.
#[tokio::test]
async fn a_download_that_fails_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let rechecked = Ran::default();
    let fake = installer(
        "windows",
        dir.path(),
        Err("not to be checked".to_owned()),
        Duration::from_secs(30),
        true,
        &rechecked,
    );

    let report = fake
        .run(RuntimeKind::Llama, |_: PathBuf| {
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

/// Off Windows nothing is downloaded, checked or run.
#[tokio::test]
async fn off_windows_there_is_nothing_to_install() {
    let dir = tempfile::tempdir().unwrap();
    let (downloaded, rechecked) = (Ran::default(), Ran::default());
    for os in ["macos", "linux"] {
        let fake = installer(
            os,
            dir.path(),
            Ok(()),
            Duration::from_secs(30),
            true,
            &rechecked,
        );
        let report = fake
            .run(
                RuntimeKind::Llama,
                download("exit 0".to_owned(), &downloaded),
            )
            .await;
        assert_eq!(report.outcome, RuntimeInstall::NotNeeded, "{os}");
    }
    assert!(!downloaded.get());
    assert!(!rechecked.get());
}
