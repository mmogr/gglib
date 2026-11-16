//! Microsoft's Visual C++ Redistributable (x64), which the Windows builds of
//! llama.cpp and stable-diffusion.cpp need and Windows does not come with:
//! where it stands, and installing it.
//!
//! The one place that knows about it: where it stands on this machine
//! (`engine_vc_state.rs`: its DLLs and its registry key), where its
//! installer comes from (`engine_vc_download.rs`), how the download is
//! checked before it runs (`engine_vc_signature.rs`), how it is run, and
//! what its exit codes mean. The OS is a value here ([`HostOs`]), not a
//! `cfg`: off Windows nothing is downloaded or run. A `#[path]` child of
//! `engine_repair.rs`.
//!
//! The installer runs only when a person has asked for it: Repair in the
//! app, or a yes to the question `gglib config <engine> install` and
//! `gglib up` ask (`gglib up --yes` answers it ahead of time). It needs
//! administrator rights, so Windows shows its own prompt for them, which
//! every surface says first.
//!
//! # From the check to the run
//!
//! What is prevented, on Windows, against another process of the same user
//! that tries to have a file run that was never checked:
//!
//! - The folder has a random name and is created so that one already there
//!   is never reused; it is then held ([`hold_folder`]) until the installer
//!   has exited, so it cannot be renamed or deleted, and a junction in its
//!   place is refused.
//! - The file is created only where nothing is, and is shared for reading
//!   only while it is written ([`create_private`]), so it cannot be renamed
//!   aside or replaced during the download.
//! - Between the download's handle closing and the file's hold being taken
//!   there is a moment in which it could be replaced; so once it is held
//!   ([`hold_unchanged`], read-only sharing, a link refused) its contents
//!   are read through the hold and must have the SHA-256 the download had.
//!   From then on, until the installer exits, nobody can write, delete or
//!   rename it, and the file checked is the file run.
//!
//! What is not prevented: a process of the same user can already write
//! gglib's own memory, or the memory of the PowerShell that checks the
//! signature, and so make any check say anything. What such a process
//! cannot make is Windows' own permission prompt, which names the publisher
//! Windows itself verified: that prompt, which the person answers, is the
//! last check, and it is not gglib's.
//!
//! # One install at a time, whoever stops waiting
//!
//! An install runs as a task of its own, which holds the one-install lock
//! and removes its folder when it ends. A caller that goes away (a page
//! reloaded during a repair) stops waiting for it and nothing more: the
//! installer finishes, and a second install is refused until it has.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tokio::fs::File;

use gglib_core::domain::RuntimeKind;
use gglib_core::host::HostOs;
use gglib_core::utils::process::bounded::{VISUAL_CPP_RUNTIME_X64_URL, run_bounded_silent};
use gglib_core::utils::process::cmd;
use gglib_core::utils::process::held::{create_private, hold_folder, hold_unchanged};

use crate::engine::validate_engine;

pub(crate) use download::download_from_microsoft;
pub(crate) use signature::installer_checked;
pub(crate) use state::{RuntimeState, runtime_state};

#[path = "engine_vc_download.rs"]
mod download;

#[path = "engine_vc_signature.rs"]
mod signature;

#[path = "engine_vc_state.rs"]
mod state;

/// What the runtime is called wherever gglib names it.
pub(super) const RUNTIME: &str = "Microsoft Visual C++ Redistributable (x64)";

/// Where Windows' question for permission may be. The service, not the
/// window clicked in, asks for it, so Windows may leave it on the taskbar.
/// The desktop app lends the service the right to come to the front just
/// before a repair (`allow_service_foreground`, #2046), but that can fail,
/// a browser tab cannot lend it, and Windows does not promise where the
/// prompt opens; so this is always said.
pub(super) const LOOK_FOR_IT: &str =
    "if no question appears, look for a flashing shield on the taskbar.";

/// How a damaged runtime is repaired: by Windows, which gglib cannot do.
pub(super) const REPAIR_IT: &str = "Repair it in Windows: Settings › Apps › Installed apps › \
     Microsoft Visual C++ v14 Redistributable (x64) (2015-2022 on an older one) › Modify › \
     Repair, then check again.";

/// The name the downloaded installer is saved under, before the OS's
/// suffix for a program.
pub(crate) const INSTALLER_STEM: &str = "vc_redist.x64";

/// The installer's documented switches: install, show nothing, and do not
/// restart Windows by itself.
const INSTALLER_SWITCHES: [&str; 3] = ["/install", "/quiet", "/norestart"];

/// How long gglib waits for the installer, including the time a person
/// takes to answer Windows' prompt.
const INSTALL_WAIT: Duration = Duration::from_mins(10);

/// What became of an install of the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInstall {
    /// Not Windows: there is no such runtime to install.
    NotNeeded,
    /// Another install of it is running in this process.
    AlreadyRunning,
    /// The download from Microsoft failed, or could not be saved; nothing
    /// was run.
    NotDownloaded,
    /// The download is not Microsoft's signed installer, or Windows could
    /// not say whose it is; it was not run.
    NotVerified,
    /// The installer exited 0: installed.
    Installed,
    /// The installer exited 3010: installed, and Windows needs a restart to
    /// finish.
    RestartNeeded,
    /// The installer exited 1638, a newer version is already installed,
    /// and its DLLs are there.
    NewerInstalled,
    /// The installer exited 1638 and the runtime's DLLs are still missing:
    /// the installed runtime is damaged.
    Damaged,
    /// The installer exited 1602, or Windows' prompt was cancelled (1223):
    /// nothing was installed.
    Declined,
    /// Windows refused the installer the rights it needs (740, 5).
    Refused,
    /// Another installation is running (1618).
    Busy,
    /// gglib stopped waiting for the installer, which may still finish.
    TimedOut,
    /// The installer could not be started, or exited with another code.
    Failed,
}

/// What an install of the runtime did for one engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RepairReport {
    pub outcome: RuntimeInstall,
    /// Whether the engine was checked again after an install that may have
    /// changed something, and works. False when it was not checked.
    pub works: bool,
    /// What happened, in the words every surface shows.
    pub says: String,
}

/// What the installer's exit `code` means ([MS-ERREF] 2.2, and the codes
/// Microsoft's Visual C++ installer documents).
pub(crate) const fn exit_outcome(code: Option<i32>) -> RuntimeInstall {
    match code {
        Some(0) => RuntimeInstall::Installed,
        // ERROR_SUCCESS_REBOOT_REQUIRED, and ERROR_SUCCESS_REBOOT_INITIATED,
        // which `/norestart` should never give.
        Some(3010 | 1641) => RuntimeInstall::RestartNeeded,
        // ERROR_PRODUCT_VERSION: whether it is there is asked next.
        Some(1638) => RuntimeInstall::NewerInstalled,
        // ERROR_INSTALL_USER_EXIT, and ERROR_CANCELLED from a prompt that
        // was answered No.
        Some(1602 | 1223) => RuntimeInstall::Declined,
        // ERROR_ELEVATION_REQUIRED and ERROR_ACCESS_DENIED.
        Some(740 | 5) => RuntimeInstall::Refused,
        // ERROR_INSTALL_ALREADY_RUNNING.
        Some(1618) => RuntimeInstall::Busy,
        _ => RuntimeInstall::Failed,
    }
}

/// The one install at a time in this process.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Holds a lock like [`INSTALLING`] until it is dropped.
pub(crate) struct Installing(&'static AtomicBool);

impl Installing {
    pub(crate) fn take(lock: &'static AtomicBool) -> Option<Self> {
        lock.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            .then_some(Self(lock))
    }
}

impl Drop for Installing {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Install the runtime for `kind`, which does not work without it.
///
/// Download it from Microsoft, check that Microsoft signed it, run it
/// quietly, and check `kind` again. Run only when a person has asked for
/// it. The install is a task of its own: a caller that stops waiting does
/// not stop it.
pub async fn install_visual_cpp_runtime(kind: RuntimeKind) -> RepairReport {
    let os = HostOs::current();
    detached(
        &INSTALLING,
        kind,
        move |dir| production(kind, os, dir),
        download_from_microsoft,
    )
    .await
}

/// The install gglib runs for `kind` on `os`, in `dir`: the real check of
/// the download, the real wait, the engine's own check afterwards.
pub(crate) fn production(
    kind: RuntimeKind,
    os: HostOs,
    dir: PathBuf,
) -> Installer<
    impl FnOnce(&Path) -> Result<(), String> + Send + 'static,
    impl FnOnce() -> bool + Send + 'static,
    impl FnOnce() -> RuntimeState + Send + 'static,
> {
    Installer {
        os,
        dir,
        verify: installer_checked,
        wait: INSTALL_WAIT,
        recheck: move || validate_engine(kind).is_ok(),
        runtime: runtime_state,
    }
}

/// Run an install made by `make` in a fresh folder, as a task of its own
/// that holds `lock` until it ends and then removes the folder; wait for
/// it. A second install while `lock` is held is refused.
pub(crate) async fn detached<V, C, R, D, F>(
    lock: &'static AtomicBool,
    kind: RuntimeKind,
    make: impl FnOnce(PathBuf) -> Installer<V, C, R> + Send + 'static,
    download: D,
) -> RepairReport
where
    V: FnOnce(&Path) -> Result<(), String> + Send + 'static,
    C: FnOnce() -> bool + Send + 'static,
    R: FnOnce() -> RuntimeState + Send + 'static,
    D: FnOnce(PathBuf, File) -> F + Send + 'static,
    F: Future<Output = Result<Digest, String>> + Send,
{
    let Some(installing) = Installing::take(lock) else {
        return report(kind, RuntimeInstall::AlreadyRunning, None, None);
    };
    let task = tokio::spawn(async move {
        let _installing = installing;
        let prepared = fresh_folder().and_then(|folder| {
            let held = hold_folder(folder.path())?;
            Ok((folder, held))
        });
        let (folder, held) = match prepared {
            Ok(prepared) => prepared,
            Err(e) => {
                let reason = format!("a folder for it could not be made and held ({e})");
                return report(kind, RuntimeInstall::NotDownloaded, Some(&reason), None);
            }
        };
        let report = make(folder.path().to_path_buf()).run(kind, download).await;
        // The folder is let go once the installer has exited, and goes with
        // the install, before the lock is let go.
        drop(held);
        drop(folder);
        report
    });
    task.await
        .unwrap_or_else(|e| report(kind, RuntimeInstall::Failed, Some(&e.to_string()), None))
}

/// A folder for one install, in this user's temporary folder: its name is
/// random, and it is created so that a folder already there is never
/// reused (another name is tried instead). It is removed when dropped.
pub(crate) fn fresh_folder() -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("gglib-visual-cpp-runtime-")
        .rand_bytes(16)
        .tempdir()
}

/// A SHA-256 digest.
pub(crate) type Digest = [u8; 32];

/// The SHA-256 of what `file` holds, read from its start.
pub(crate) fn digest_of(mut file: &std::fs::File) -> io::Result<Digest> {
    use sha2::Digest as _;
    use std::io::{Read as _, Seek as _};
    file.seek(io::SeekFrom::Start(0))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hasher.finalize().into());
        }
        hasher.update(&buffer[..read]);
    }
}

/// The parts of an install of the runtime, each one a test can replace.
pub(crate) struct Installer<V, C, R> {
    /// The OS this is.
    pub os: HostOs,
    /// The folder the installer is downloaded to, made for this install.
    pub dir: PathBuf,
    /// Whether the download at a path may be run, or why not.
    pub verify: V,
    /// How long gglib waits for the installer.
    pub wait: Duration,
    /// Whether the engine works, asked after an install that may have
    /// changed something.
    pub recheck: C,
    /// Where the runtime stands, asked when the installer says a newer one
    /// is installed.
    pub runtime: R,
}

impl<V, C, R> Installer<V, C, R>
where
    V: FnOnce(&Path) -> Result<(), String> + Send + 'static,
    C: FnOnce() -> bool + Send + 'static,
    R: FnOnce() -> RuntimeState + Send + 'static,
{
    /// Create the installer's file, which must not be there yet, and have
    /// `download` write it and say the SHA-256 of what it wrote; then hold
    /// it, check it is what was written, check it, run it and check `kind`
    /// again.
    pub(crate) async fn run<D, F>(self, kind: RuntimeKind, download: D) -> RepairReport
    where
        D: FnOnce(PathBuf, File) -> F,
        F: Future<Output = Result<Digest, String>>,
    {
        if self.os != HostOs::Windows {
            return report(kind, RuntimeInstall::NotNeeded, None, None);
        }
        let path = self
            .dir
            .join(format!("{INSTALLER_STEM}{}", std::env::consts::EXE_SUFFIX));
        let downloaded = match create_private(&path) {
            Ok(file) => download(path.clone(), File::from_std(file)).await,
            Err(e) => Err(format!("{} could not be created ({e})", path.display())),
        };
        let digest = match downloaded {
            Ok(digest) => digest,
            Err(reason) => {
                return report(kind, RuntimeInstall::NotDownloaded, Some(&reason), None);
            }
        };
        tokio::task::spawn_blocking(move || self.run_downloaded(kind, &path, digest))
            .await
            .unwrap_or_else(|e| report(kind, RuntimeInstall::Failed, Some(&e.to_string()), None))
    }

    /// Hold the installer at `path`, check it holds what was downloaded
    /// (`digest`), check it, run it, and check `kind` again; let it go once
    /// it has exited.
    fn run_downloaded(self, kind: RuntimeKind, path: &Path, digest: Digest) -> RepairReport {
        let held = match hold_unchanged(path) {
            Ok(held) => held,
            Err(e) => {
                let reason = format!("it could not be held while it was checked ({e})");
                return report(kind, RuntimeInstall::NotVerified, Some(&reason), None);
            }
        };
        match digest_of(&held) {
            Ok(held_digest) if held_digest == digest => {}
            Ok(_) => {
                let reason = "the file was changed after it was downloaded";
                return report(kind, RuntimeInstall::NotVerified, Some(reason), None);
            }
            Err(e) => {
                let reason = format!("it could not be read back ({e})");
                return report(kind, RuntimeInstall::NotVerified, Some(&reason), None);
            }
        }
        let Self {
            verify,
            wait,
            recheck,
            runtime,
            ..
        } = self;
        if let Err(reason) = verify(path) {
            return report(kind, RuntimeInstall::NotVerified, Some(&reason), None);
        }
        let ran = run_bounded_silent(cmd(path).args(INSTALLER_SWITCHES), wait);
        drop(held);
        after_exit(kind, ran.map(|status| status.code()), runtime, recheck)
    }
}

/// What an install that `ran` (the installer's exit code, or why it did not
/// exit) did for `kind`: where that says more, `runtime` is asked where the
/// runtime stands, and `recheck` whether `kind` works.
pub(crate) fn after_exit(
    kind: RuntimeKind,
    ran: io::Result<Option<i32>>,
    runtime: impl FnOnce() -> RuntimeState,
    recheck: impl FnOnce() -> bool,
) -> RepairReport {
    let (outcome, detail) = match ran {
        Ok(code) => (exit_outcome(code), code.map(|code| code.to_string())),
        Err(e) if e.kind() == io::ErrorKind::TimedOut => (RuntimeInstall::TimedOut, None),
        Err(e) if matches!(e.raw_os_error(), Some(740 | 5)) => (
            RuntimeInstall::Refused,
            e.raw_os_error().map(|code| code.to_string()),
        ),
        Err(e) => (RuntimeInstall::Failed, Some(e.to_string())),
    };
    // 1638 says a newer runtime is installed; one was offered because
    // its DLLs were missing, so if they still are, it is damaged and
    // another install would answer 1638 again.
    let outcome = match outcome {
        RuntimeInstall::NewerInstalled if runtime() != RuntimeState::Installed => {
            RuntimeInstall::Damaged
        }
        other => other,
    };
    let changed = matches!(
        outcome,
        RuntimeInstall::Installed | RuntimeInstall::RestartNeeded | RuntimeInstall::NewerInstalled
    );
    let works = changed.then(recheck);
    report(kind, outcome, detail.as_deref(), works)
}

/// The report of `outcome` for `kind`: `detail` is the reason a step failed,
/// or the installer's exit code; `works` whether `kind` works when it was
/// checked again.
pub(crate) fn report(
    kind: RuntimeKind,
    outcome: RuntimeInstall,
    detail: Option<&str>,
    works: Option<bool>,
) -> RepairReport {
    RepairReport {
        outcome,
        works: works == Some(true),
        says: words(kind, outcome, detail, works),
    }
}

/// What a surface says of `outcome` for `kind`.
fn words(
    kind: RuntimeKind,
    outcome: RuntimeInstall,
    detail: Option<&str>,
    works: Option<bool>,
) -> String {
    let server = kind.server_name();
    let product = kind.label();
    let detail = detail.unwrap_or("no reason given");
    let by_hand = format!("It can be installed by hand from {VISUAL_CPP_RUNTIME_X64_URL}");
    let then = match works {
        Some(true) => format!(" {server} works now."),
        Some(false) => format!(
            " {server} still cannot start, so a file that came with {product} may be missing: \
             reinstall {product}."
        ),
        None => String::new(),
    };
    match outcome {
        RuntimeInstall::NotNeeded => {
            format!("Nothing to install: the {RUNTIME} is for Windows only.")
        }
        RuntimeInstall::AlreadyRunning => {
            format!("The {RUNTIME} is already being installed. Wait for it to finish.")
        }
        RuntimeInstall::NotDownloaded => format!(
            "Could not download the {RUNTIME} from Microsoft ({detail}). Nothing was installed."
        ),
        RuntimeInstall::NotVerified => {
            format!("The download was not run: {detail}. Nothing was installed. {by_hand}.")
        }
        RuntimeInstall::Installed => format!("Installed the {RUNTIME}.{then}"),
        RuntimeInstall::RestartNeeded if works == Some(false) => format!(
            "Installed the {RUNTIME}; Windows needs a restart to finish it. {server} still \
             cannot start: restart Windows, then try again."
        ),
        RuntimeInstall::RestartNeeded => {
            format!("Installed the {RUNTIME}; Windows needs a restart to finish it.{then}")
        }
        RuntimeInstall::NewerInstalled => {
            format!("A newer {RUNTIME} is already installed.{then}")
        }
        RuntimeInstall::Damaged => format!(
            "Windows says a newer {RUNTIME} is already installed, but its files are missing, \
             so it is damaged and installing it again does not help. {REPAIR_IT}"
        ),
        RuntimeInstall::Declined => {
            "Nothing was installed: Windows' permission was not given.".to_owned()
        }
        RuntimeInstall::Refused => format!(
            "Windows refused the {RUNTIME} installer the permission it needs ({detail}). \
             Nothing was installed."
        ),
        RuntimeInstall::Busy => format!(
            "Windows is installing something else, so the {RUNTIME} could not be installed \
             now. Wait for that to finish, then try again."
        ),
        RuntimeInstall::TimedOut => format!(
            "gglib stopped waiting for the {RUNTIME} installer after {} minutes. Windows' \
             question for permission was probably waiting behind a flashing shield on the \
             taskbar; the installer may still finish if it is answered. Check again in a \
             while, or install it by hand from {VISUAL_CPP_RUNTIME_X64_URL}.",
            INSTALL_WAIT.as_secs() / 60
        ),
        RuntimeInstall::Failed => {
            format!("The {RUNTIME} installer did not finish ({detail}). {by_hand}.")
        }
    }
}

#[cfg(test)]
#[path = "engine_vc_runtime_tests.rs"]
mod tests;
