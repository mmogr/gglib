//! The engines gglib installs, asked the same questions.
//!
//! llama.cpp's `llama-server` and stable-diffusion.cpp's `sd-server` are
//! installed the same way: a pinned release, downloaded or built from
//! source into a folder of the engine's own, with a record of how it got
//! there. Each question here is asked of either by its [`RuntimeKind`], so
//! the two cannot be answered differently.

use std::path::{Path, PathBuf};

use serde::Serialize;

use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{
    gglib_data_dir, llama_config_path, llama_cpp_dir, llama_server_path, sd_config_path,
    sd_cpp_dir, sd_data_dir, sd_server_path,
};

pub use crate::binary_install::PinStanding;
use crate::binary_install::{
    Recorded, ReleaseSelector, ReleaseSpec, installed_release, leftovers, recorded_install,
    resolve_selector, standing,
};

/// Where an engine's install stands against its pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PinState {
    /// Whether there is anything for an install to do.
    pub standing: PinStanding,
    /// The release installed, as it was compared with the pin: a tag, or a
    /// source build's short commit. Null with no record of the install.
    pub installed_release: Option<String>,
    /// The release an install would fetch: the pin, or the tag an override
    /// names. Null when the override follows upstream's newest.
    pub target_release: Option<String>,
    /// Whether the recorded install was a source build. Null with no record.
    pub built: Option<bool>,
    /// Whether an install over this one builds from source again unless told
    /// otherwise: a source build of a release does, and anything else takes
    /// the download where one fits. A source build recorded by commit, from
    /// before builds were of a release, takes the default.
    pub rebuilds: bool,
    /// Whether the install a missing engine calls for, or the update an older
    /// one does, is a download, which the daemon and the app can run. When it
    /// is a source build (one of a release was there, a GPU build would
    /// otherwise become a CPU one, or this machine has no download), it is
    /// the command line's to run. False whenever there is nothing to install.
    pub update_downloads: bool,
}

/// How an install of an engine goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPlan {
    /// Download this machine's build of the release: `description` names it,
    /// and `warning` is what to say about it first, such as a CPU build's
    /// slowness.
    Download {
        description: String,
        warning: Option<String>,
    },
    /// Build from source. `no_download` says why when nobody asked for a
    /// build: this machine has no download.
    Build { no_download: Option<String> },
}

/// How to install `kind` over what `state` describes.
///
/// A build when `build` asks for one, or when what is installed is a source
/// build of a release and `force` does not ask for the default instead; a
/// download where this machine has one; a build otherwise. A source build is
/// never replaced by a download that runs on the CPU alone, the one that
/// comes with a warning, unless `force` asks: a build made for a GPU would
/// otherwise turn into a slow one on an update nobody chose.
#[must_use]
pub fn plan_install(kind: RuntimeKind, state: &PinState, force: bool, build: bool) -> InstallPlan {
    plan_with(state, force, build, || download(kind))
}

/// [`plan_install`] with this machine's download asked of `prebuilt`, which
/// is asked only when nothing before it has decided.
pub(crate) fn plan_with(
    state: &PinState,
    force: bool,
    build: bool,
    prebuilt: impl FnOnce() -> Result<(String, Option<String>), String>,
) -> InstallPlan {
    if build || (state.rebuilds && !force) {
        return InstallPlan::Build { no_download: None };
    }
    match prebuilt() {
        Ok((_, Some(_))) if state.built == Some(true) && !force => {
            InstallPlan::Build { no_download: None }
        }
        Ok((description, warning)) => InstallPlan::Download {
            description,
            warning,
        },
        Err(reason) => InstallPlan::Build {
            no_download: Some(reason),
        },
    }
}

/// This machine's download of `kind`, as its description and the warning it
/// comes with, or why there is none.
pub fn download(kind: RuntimeKind) -> Result<(String, Option<String>), String> {
    match kind {
        RuntimeKind::Llama => match crate::llama::check_prebuilt_availability() {
            crate::llama::PrebuiltAvailability::Available {
                description,
                warning,
                ..
            } => Ok((description, warning.map(str::to_owned))),
            crate::llama::PrebuiltAvailability::NotAvailable { reason } => Err(reason),
        },
        RuntimeKind::StableDiffusion => crate::sd::check_sd_prebuilt_availability().map(|asset| {
            (
                asset.description.to_owned(),
                asset.warning.map(str::to_owned),
            )
        }),
    }
}

/// Where `kind`'s install stands against its pin, from its binary, its
/// record, its checkout and its override. Only paths that cannot be
/// resolved are an `Err`; an unreadable record counts as none.
pub fn pin_state(kind: RuntimeKind) -> anyhow::Result<PinState> {
    let paths = EnginePaths::of(kind)?;
    let mut state = pin_state_at(
        &paths.binary,
        recorded_install(&paths.record).as_ref(),
        &paths.checkout,
        &resolve_selector(release_spec(kind)),
    );
    state.update_downloads = update_downloads(kind, &state);
    Ok(state)
}

/// Whether the install or update `state` calls for is a download on this
/// machine: [`PinState::update_downloads`].
pub(crate) fn update_downloads(kind: RuntimeKind, state: &PinState) -> bool {
    calls_for_download(state, || download(kind))
}

/// [`update_downloads`] with this machine's download asked of `prebuilt`.
pub(crate) fn calls_for_download(
    state: &PinState,
    prebuilt: impl FnOnce() -> Result<(String, Option<String>), String>,
) -> bool {
    matches!(state.standing, PinStanding::Missing | PinStanding::Older)
        && matches!(
            plan_with(state, false, false, prebuilt),
            InstallPlan::Download { .. }
        )
}

/// [`pin_state`] for a binary, what a record says, a checkout and a
/// selector given; whether its update downloads is left false, for the
/// caller that knows the engine to ask.
pub(crate) fn pin_state_at(
    binary: &Path,
    recorded: Option<&Recorded>,
    checkout: &Path,
    selector: &ReleaseSelector,
) -> PinState {
    let installed = recorded.map(|recorded| installed_release(recorded, checkout));
    PinState {
        standing: standing(binary.is_file(), installed.as_ref(), selector),
        installed_release: installed.map(|installed| installed.release),
        target_release: match selector {
            ReleaseSelector::Tag(tag) => Some(tag.clone()),
            ReleaseSelector::Latest => None,
        },
        built: recorded.map(Recorded::built),
        rebuilds: recorded.is_some_and(|recorded| recorded.built() && !recorded.names_a_commit()),
        update_downloads: false,
    }
}

/// What a status a person reads says of `state`; `None` with nothing
/// installed, where the status already says so.
#[must_use]
pub fn standing_line(state: &PinState) -> Option<String> {
    let pin = state
        .target_release
        .as_deref()
        .unwrap_or("upstream's newest");
    let installed = state.installed_release.as_deref().unwrap_or("unknown");
    Some(match state.standing {
        PinStanding::Missing => return None,
        PinStanding::AtPin => format!("Pinned release: {pin}, which is the one installed."),
        PinStanding::Older => {
            format!("Pinned release: {pin}. The one installed, {installed}, is older.")
        }
        PinStanding::Newer => {
            format!("Pinned release: {pin}. The one installed, {installed}, is newer.")
        }
        PinStanding::Unknown if state.target_release.is_none() => {
            "Following upstream's newest release, which is not compared offline.".to_owned()
        }
        PinStanding::Unknown if state.installed_release.is_none() => {
            format!("Pinned release: {pin}. There is no record of what is installed.")
        }
        PinStanding::Unknown => format!(
            "Pinned release: {pin}. The one installed, {installed}, cannot be placed against it \
             offline."
        ),
    })
}

/// What an uninstall removed.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UninstallOutcome {
    /// False when there was nothing to remove; `removedPaths` is then empty.
    pub was_installed: bool,
    pub removed_paths: Vec<String>,
}

/// Remove everything gglib installed for `kind`, each by name.
///
/// That is the binaries and their libraries, the record, a source checkout
/// with what an unfinished build left beside it, and sd's download folder.
/// The engine's folder (`.llama/`, `.sd/`) goes too once that leaves it
/// empty. Anything else in it was put there by something other than gglib,
/// such as another tool's `~/.llama` when the data folder is home, and
/// stays. Unconditional; the confirmation, and the refusal while the engine
/// serves a model, belong to the caller.
pub fn uninstall(kind: RuntimeKind) -> anyhow::Result<UninstallOutcome> {
    let paths = EnginePaths::of(kind)?;
    uninstall_from(&paths.dir, &paths.installed)
}

/// Whether there is anything for [`uninstall`] to remove.
pub fn files_present(kind: RuntimeKind) -> anyhow::Result<bool> {
    Ok(EnginePaths::of(kind)?
        .installed
        .iter()
        .any(|path| exists(path)))
}

/// [`uninstall`] of the paths `installed`, in the engine's folder `dir`.
pub(crate) fn uninstall_from(
    dir: &Path,
    installed: &[PathBuf],
) -> anyhow::Result<UninstallOutcome> {
    let mut removed_paths = Vec::new();
    for path in installed {
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if meta.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
        removed_paths.push(path.display().to_string());
    }
    // `remove_dir` takes only an empty folder, which is the point: what is
    // left in a folder that is not, is not gglib's. A folder this removal did
    // not empty, one another tool made, is not touched at all.
    if !removed_paths.is_empty() && std::fs::remove_dir(dir).is_err() {
        tracing::debug!(dir = %dir.display(), "kept the engine's folder: not empty, or not there");
    }
    Ok(UninstallOutcome {
        was_installed: !removed_paths.is_empty(),
        removed_paths,
    })
}

/// Whether anything is at `path`, a dangling link included.
fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Where an engine keeps what an install made.
struct EnginePaths {
    /// The engine's own folder, which holds everything below.
    dir: PathBuf,
    binary: PathBuf,
    record: PathBuf,
    checkout: PathBuf,
    /// Every path an install puts in the folder: what an uninstall removes.
    installed: Vec<PathBuf>,
}

impl EnginePaths {
    fn of(kind: RuntimeKind) -> anyhow::Result<Self> {
        let (dir, binary, record, checkout) = match kind {
            RuntimeKind::Llama => (
                gglib_data_dir()?,
                llama_server_path()?,
                llama_config_path()?,
                llama_cpp_dir()?,
            ),
            RuntimeKind::StableDiffusion => (
                sd_data_dir()?,
                sd_server_path()?,
                sd_config_path()?,
                sd_cpp_dir()?,
            ),
        };
        let bin = binary
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("{} has no folder", binary.display()))?;
        let mut installed = vec![bin, record.clone(), checkout.clone()];
        installed.extend(leftovers(&checkout));
        // llama.cpp stages its archive in the data root's `downloads/`,
        // outside `.llama/`, and its installer removes it either way; sd
        // stages its own in its folder.
        if kind == RuntimeKind::StableDiffusion {
            installed.push(crate::sd::sd_download_dir()?);
        }
        Ok(Self {
            dir,
            binary,
            record,
            checkout,
            installed,
        })
    }
}

/// The release `kind` is published as.
fn release_spec(kind: RuntimeKind) -> &'static ReleaseSpec {
    match kind {
        RuntimeKind::Llama => &crate::llama::LLAMA_RELEASE,
        RuntimeKind::StableDiffusion => &crate::sd::SD_RELEASE,
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
