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
    llama_config_path, llama_cpp_dir, llama_server_path, sd_config_path, sd_cpp_dir, sd_server_path,
};

pub use crate::binary_install::PinStanding;
use crate::binary_install::{
    Recorded, ReleaseSelector, ReleaseSpec, installed_release, resolve_selector, standing,
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
}

/// Where `kind`'s install stands against its pin, from its binary, its
/// record, its checkout and its override. Only paths that cannot be
/// resolved are an `Err`; an unreadable record counts as none.
pub fn pin_state(kind: RuntimeKind) -> anyhow::Result<PinState> {
    let paths = EnginePaths::of(kind)?;
    Ok(pin_state_at(
        &paths.binary,
        recorded(kind, &paths.record).as_ref(),
        &paths.checkout,
        &resolve_selector(release_spec(kind)),
    ))
}

/// [`pin_state`] for a binary, what a record says, a checkout and a
/// selector given.
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

/// Where an engine keeps what an install made.
struct EnginePaths {
    binary: PathBuf,
    record: PathBuf,
    checkout: PathBuf,
}

impl EnginePaths {
    fn of(kind: RuntimeKind) -> anyhow::Result<Self> {
        Ok(match kind {
            RuntimeKind::Llama => Self {
                binary: llama_server_path()?,
                record: llama_config_path()?,
                checkout: llama_cpp_dir()?,
            },
            RuntimeKind::StableDiffusion => Self {
                binary: sd_server_path()?,
                record: sd_config_path()?,
                checkout: sd_cpp_dir()?,
            },
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

/// What `kind`'s record at `path` says was installed; `None` with no record
/// or one that cannot be read.
fn recorded(kind: RuntimeKind, path: &Path) -> Option<Recorded> {
    match kind {
        RuntimeKind::Llama => crate::llama::recorded_install(path),
        RuntimeKind::StableDiffusion => crate::sd::recorded_install(path),
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
