//! What gglib can do about an engine that is not working, for either
//! engine: which repair a failure calls for ([`Fix`]), with its words.
//!
//! A reinstall is the install's. The one repair that changes the machine
//! rather than the engine, installing Microsoft's Visual C++ Redistributable
//! (x64), is `engine_vc_runtime.rs`'s, the one place that knows about it.
//! How a program that cannot load a library ends is `failure_kind`'s to
//! read; this reads only the [`Failure`] it gives. The OS is a value here
//! ([`HostOs`]), not a `cfg`, so every branch is tested on every OS: on
//! macOS and Linux the runtime is never offered. A `#[path]` child of
//! `engine.rs`.

use serde::Serialize;

use gglib_core::domain::RuntimeKind;
use gglib_core::host::HostOs;
use gglib_core::utils::process::bounded::Failure;

use super::{NotWorking, validate_engine};

pub use runtime::{RepairReport, RuntimeInstall, install_visual_cpp_runtime};
use runtime::{RuntimeState, runtime_state};

#[path = "engine_vc_runtime.rs"]
mod runtime;

/// A repair for an engine that is not working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// Install Microsoft's Visual C++ Redistributable (x64): on Windows the
    /// engine cannot load a library, and the runtime is not installed.
    InstallVisualCppRuntime,
    /// The runtime is installed but its files are missing: it is damaged,
    /// and Windows' own repair of it is the fix. gglib cannot make it.
    RepairVisualCppRuntime,
    /// Install the engine again: what is there is damaged or incomplete.
    Reinstall,
}

impl Fix {
    /// What is said of it before anyone is asked, for `kind`.
    #[must_use]
    pub fn offer(self, kind: RuntimeKind) -> String {
        match self {
            Self::InstallVisualCppRuntime => format!(
                "gglib can install the {} from Microsoft. Windows will ask for permission; {}",
                runtime::RUNTIME,
                runtime::LOOK_FOR_IT
            ),
            Self::RepairVisualCppRuntime => format!(
                "The {} is installed but damaged: its files are missing. {}",
                runtime::RUNTIME,
                runtime::REPAIR_IT
            ),
            Self::Reinstall => format!(
                "gglib can download {} again and reinstall it.",
                kind.label()
            ),
        }
    }

    /// What is said while it is being made, when there is something to say
    /// beyond progress: where Windows' question for permission may be.
    #[must_use]
    pub fn waiting(self) -> Option<String> {
        match self {
            Self::InstallVisualCppRuntime => Some(format!(
                "Waiting for Windows' permission to install the {}; {}",
                runtime::RUNTIME,
                runtime::LOOK_FOR_IT
            )),
            Self::RepairVisualCppRuntime | Self::Reinstall => None,
        }
    }

    /// The word on the button that makes it, when gglib makes it; `None`
    /// when the fix is a person's to make, as Windows' repair of a damaged
    /// runtime is.
    #[must_use]
    pub const fn action(self) -> Option<&'static str> {
        match self {
            Self::InstallVisualCppRuntime | Self::Reinstall => Some("Repair"),
            Self::RepairVisualCppRuntime => None,
        }
    }
}

/// A repair as a status carries it: which, what is said about it, and the
/// button that makes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FixOffer {
    pub fix: Fix,
    /// [`Fix::offer`]: shown beside the button, and before the command
    /// line's question.
    pub offer: String,
    /// [`Fix::action`]: the button's word; null when there is no button,
    /// because gglib cannot make the fix.
    pub action: Option<String>,
    /// [`Fix::waiting`]: shown while the fix is being made; null when
    /// progress says enough.
    pub waiting: Option<String>,
}

impl FixOffer {
    /// `fix` for `kind`, with its words.
    #[must_use]
    pub fn new(fix: Fix, kind: RuntimeKind) -> Self {
        Self {
            fix,
            offer: fix.offer(kind),
            action: fix.action().map(str::to_owned),
            waiting: fix.waiting(),
        }
    }
}

impl NotWorking {
    /// The repair for this on the OS this is, if there is one.
    #[must_use]
    pub fn fix(&self) -> Option<Fix> {
        fix_for(&self.failure, HostOs::current(), runtime_state)
    }
}

/// The repair for `failure` on `os`. `runtime` says where the Visual C++
/// runtime stands, and is asked only of a Windows library that was not
/// found.
pub(crate) fn fix_for(
    failure: &Failure,
    os: HostOs,
    runtime: impl FnOnce() -> RuntimeState,
) -> Option<Fix> {
    match failure {
        // Windows does not say which DLL. With the runtime missing it is the
        // likeliest; with it there, it is one that came with the download;
        // with it registered and its files gone, the runtime is damaged.
        Failure::MissingLibrary { name: None } if os == HostOs::Windows => Some(match runtime() {
            RuntimeState::Installed => Fix::Reinstall,
            RuntimeState::Damaged => Fix::RepairVisualCppRuntime,
            RuntimeState::Missing => Fix::InstallVisualCppRuntime,
        }),
        // What the engine's check already tells a person to reinstall for.
        Failure::NotExecutable | Failure::Crashed { .. } | Failure::Failed { .. } => {
            Some(Fix::Reinstall)
        }
        // A named library may be the system's, which a reinstall does not
        // bring, and one the system has too old a copy of is; a time-out or
        // a refusal to start is not the download's.
        _ => None,
    }
}

/// The repair `kind` calls for now: `None` when it works, or when there is
/// none for why it does not.
///
/// # Errors
///
/// Its path cannot be resolved.
pub fn current_fix(kind: RuntimeKind) -> anyhow::Result<Option<Fix>> {
    let Err(e) = validate_engine(kind) else {
        return Ok(None);
    };
    let fix = e.downcast_ref::<NotWorking>().map(NotWorking::fix);
    fix.ok_or(e)
}

#[cfg(test)]
#[path = "engine_repair_tests.rs"]
mod tests;
