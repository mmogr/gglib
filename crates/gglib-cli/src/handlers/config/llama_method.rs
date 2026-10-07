//! How llama.cpp gets onto this machine, decided once.
//!
//! [`choose_install_method`] picks a download or a source build, and
//! [`install_by`] carries the choice out, a failed download falling back to a
//! build. `config llama install` and `rebuild` go through both, and so does
//! the install a first `gglib serve` or `gglib up` offers (`llama_ensure`).

use anyhow::Result;
use gglib_runtime::llama::PrebuiltAvailability;

/// The acceleration flags of `config llama install`: `--cuda`, `--metal` and
/// `--vulkan`. None set leaves the choice to detection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AccelerationFlags {
    pub(super) cuda: bool,
    pub(super) metal: bool,
    pub(super) vulkan: bool,
}

/// How llama.cpp gets onto this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InstallMethod {
    /// Download the release built for this platform, described as the
    /// release names it. A download that fails falls back to a build.
    Prebuilt { description: String },
    /// Build from source.
    Source {
        /// Why there is no download for this platform, when that is the
        /// reason for building: no flag asked for a build, and gglib is not
        /// running from a checkout.
        no_prebuilt: Option<String>,
    },
}

/// Choose between a download and a source build.
///
/// A build when a flag asks for one (`build`, which is `--build`, or an
/// acceleration, which only a build can honour), when gglib runs from its
/// source checkout, and when this platform has no pre-built release. A
/// download otherwise. `prebuilt` is asked only when nothing before it has
/// decided.
pub(super) fn choose_install_method(
    build: bool,
    acceleration: AccelerationFlags,
    from_checkout: bool,
    prebuilt: impl FnOnce() -> PrebuiltAvailability,
) -> InstallMethod {
    let AccelerationFlags {
        cuda,
        metal,
        vulkan,
    } = acceleration;
    if build || cuda || metal || vulkan || from_checkout {
        return InstallMethod::Source { no_prebuilt: None };
    }
    match prebuilt() {
        PrebuiltAvailability::Available { description, .. } => {
            InstallMethod::Prebuilt { description }
        }
        PrebuiltAvailability::NotAvailable { reason } => InstallMethod::Source {
            no_prebuilt: Some(reason),
        },
    }
}

/// Install by `method`: `download` for a pre-built release, and `build` for
/// a source build or after a download that failed.
pub(super) async fn install_by(
    method: &InstallMethod,
    download: impl AsyncFnOnce() -> Result<()>,
    build: impl AsyncFnOnce() -> Result<()>,
) -> Result<()> {
    if matches!(method, InstallMethod::Prebuilt { .. }) {
        match download().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                println!();
                println!("⚠️  Failed to download pre-built binaries: {e}");
                println!("Falling back to building from source...");
                println!();
            }
        }
    }
    build().await
}

#[cfg(test)]
#[path = "llama_method_tests.rs"]
mod tests;
