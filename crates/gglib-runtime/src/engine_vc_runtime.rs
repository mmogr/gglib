//! Microsoft's Visual C++ Redistributable (x64): the types the tests are
//! written against, with nothing installed yet.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use gglib_core::domain::RuntimeKind;

/// The name the downloaded installer is saved under.
pub(crate) const INSTALLER_STEM: &str = "vc_redist.x64";

/// Whether every one of the runtime's DLLs is in `system`.
pub(crate) const fn runtime_dlls_in(_system: &Path) -> bool {
    false
}

/// What became of an install of the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInstall {
    /// Not Windows: there is no such runtime to install.
    NotNeeded,
    /// Another install of it is running in this process.
    AlreadyRunning,
    /// The download from Microsoft failed; nothing was run.
    NotDownloaded,
    /// The download is not Microsoft's signed installer; it was not run.
    NotVerified,
    /// The installer exited 0.
    Installed,
    /// The installer exited 3010.
    RestartNeeded,
    /// The installer exited 1638.
    NewerInstalled,
    /// The installer exited 1602.
    Declined,
    /// The installer did not finish in time.
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
    /// Whether the engine was checked again, and works.
    pub works: bool,
    /// What happened, in the words every surface shows.
    pub says: String,
}

/// What the installer's exit `code` means.
pub(crate) const fn exit_outcome(_code: Option<i32>) -> RuntimeInstall {
    RuntimeInstall::Failed
}

/// One install at a time in this process.
pub(crate) struct Installing;

impl Installing {
    pub(crate) const fn take() -> Option<Self> {
        Some(Self)
    }
}

/// Install the runtime for `kind`.
pub async fn install_visual_cpp_runtime(kind: RuntimeKind) -> RepairReport {
    report(kind, RuntimeInstall::NotNeeded, None, None)
}

/// The parts of an install of the runtime, each one a test can replace.
pub(crate) struct Installer<V, C> {
    pub os: &'static str,
    pub dir: PathBuf,
    pub verify: V,
    pub wait: Duration,
    pub recheck: C,
}

impl<V, C> Installer<V, C>
where
    V: FnOnce(&Path) -> Result<(), String> + Send + 'static,
    C: FnOnce() -> bool + Send + 'static,
{
    /// Download the installer with `download`, then check it, run it and
    /// check `kind` again.
    pub(crate) async fn run<D, F>(self, kind: RuntimeKind, _download: D) -> RepairReport
    where
        D: FnOnce(PathBuf) -> F,
        F: Future<Output = Result<(), String>>,
    {
        report(kind, RuntimeInstall::NotNeeded, None, None)
    }
}

/// The report of `outcome` for `kind`.
pub(crate) fn report(
    _kind: RuntimeKind,
    outcome: RuntimeInstall,
    _detail: Option<&str>,
    _works: Option<bool>,
) -> RepairReport {
    RepairReport {
        outcome,
        works: false,
        says: String::new(),
    }
}

/// What Windows says of a file's Authenticode signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signature {
    pub status: String,
    pub subject: Option<String>,
}

/// Whether `signature` is a valid one of Microsoft's, or why not.
pub(crate) const fn microsoft_signed(_signature: &Signature) -> Result<(), String> {
    Ok(())
}

/// What the signature check printed, read.
pub(crate) fn parse_signature(_printed: &str) -> Signature {
    Signature {
        status: String::new(),
        subject: None,
    }
}

#[cfg(test)]
#[path = "engine_vc_runtime_tests.rs"]
mod tests;
