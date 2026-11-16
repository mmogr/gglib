//! What gglib can do about an engine that is not working: the types the
//! tests are written against, with nothing decided yet.

use serde::Serialize;

use gglib_core::domain::RuntimeKind;
use gglib_core::utils::process::bounded::Failure;

use super::NotWorking;

pub use runtime::{RepairReport, RuntimeInstall, install_visual_cpp_runtime};

#[path = "engine_vc_runtime.rs"]
mod runtime;

/// A repair gglib can make for an engine that is not working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// Install Microsoft's Visual C++ Redistributable (x64).
    InstallVisualCppRuntime,
    /// Install the engine again.
    Reinstall,
}

impl Fix {
    /// What gglib says it can do, for `kind`.
    #[must_use]
    pub fn offer(self, _kind: RuntimeKind) -> String {
        String::new()
    }
}

/// A repair as a status carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FixOffer {
    pub fix: Fix,
    /// [`Fix::offer`].
    pub offer: String,
}

impl FixOffer {
    /// `fix` for `kind`, with its words.
    #[must_use]
    pub fn new(fix: Fix, kind: RuntimeKind) -> Self {
        Self {
            fix,
            offer: fix.offer(kind),
        }
    }
}

impl NotWorking {
    /// The repair gglib can make for this, if it has one.
    #[must_use]
    pub fn fix(&self) -> Option<Fix> {
        fix_for(&self.failure, std::env::consts::OS, || false)
    }
}

/// The repair for `failure` on `os`.
pub(crate) fn fix_for(
    _failure: &Failure,
    _os: &str,
    _runtime_installed: impl FnOnce() -> bool,
) -> Option<Fix> {
    None
}

/// The repair `kind` calls for now.
///
/// # Errors
///
/// Never, yet.
pub fn current_fix(_kind: RuntimeKind) -> anyhow::Result<Option<Fix>> {
    Ok(None)
}

#[cfg(test)]
#[path = "engine_repair_tests.rs"]
mod tests;
