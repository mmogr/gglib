//! What is installed for image generation, as data.

use gglib_core::paths::{sd_config_path, sd_cpp_dir, sd_server_path};
use serde::Serialize;
use std::path::Path;

use super::release::{PINNED_SD_RELEASE, SD_RELEASE};
use crate::binary_install::{InstallRecord, recorded_install};
use crate::binary_install::{ReleaseSelector, resolve_selector};
use crate::engine::{
    Fix, PinState, first_line, pin_state_at, update_downloads, validate_engine_at,
};
use gglib_core::domain::RuntimeKind;

/// Everything a status report says about the installed `sd-server`.
///
/// Two versions are reported and kept apart, as llama.cpp's status does:
/// [`Self::release`] is what the install recorded, [`Self::version_line`] is
/// what the binary says when asked. They disagree for a binary put there by
/// hand, which is what someone debugging a launch needs to see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdStatus {
    /// Whether the binary is there.
    pub installed: bool,
    /// Why the binary that is there does not work, with what to do; null
    /// when it answers `--version` or is not there. The check is
    /// llama.cpp's too.
    pub health_error: Option<String>,
    /// What gglib can do about it, when it does not work for a reason gglib
    /// can repair.
    pub fix: Option<Fix>,
    pub binary_path: String,
    pub config_path: String,
    /// The release gglib installs by default.
    pub pinned_release: String,
    /// `prebuilt` or `source`, from the record; null with no record.
    pub install_type: Option<String>,
    /// The release the record names (a tag, or a short commit).
    pub release: Option<String>,
    /// The platform build a download chose, or the acceleration a source
    /// build compiled in.
    pub platform: Option<String>,
    /// When it was installed or built, as RFC 3339.
    pub installed_at: Option<String>,
    /// Set when a record exists but could not be read.
    pub record_error: Option<String>,
    /// The binary's own first line for `--version`
    /// (`stable-diffusion.cpp version unknown, commit 228c707`).
    pub version_line: Option<String>,
    /// The commit that line names.
    pub commit: Option<String>,
    /// Where the install stands against the pinned release.
    pub pin: PinState,
}

/// Inspect the installed `sd-server`, if any. Degraded states are values;
/// only a data directory that cannot be resolved is an `Err`.
pub fn sd_status() -> anyhow::Result<SdStatus> {
    let mut status = sd_status_at(
        &sd_server_path()?,
        &sd_config_path()?,
        &sd_cpp_dir()?,
        &resolve_selector(&SD_RELEASE),
    );
    status.pin.update_downloads = update_downloads(RuntimeKind::StableDiffusion, &status.pin);
    Ok(status)
}

/// [`sd_status`] for a binary, a record and a source checkout at the paths
/// given, and the release `selector` would install.
pub(crate) fn sd_status_at(
    binary_path: &Path,
    config_path: &Path,
    checkout: &Path,
    selector: &ReleaseSelector,
) -> SdStatus {
    let mut status = SdStatus {
        installed: binary_path.is_file(),
        health_error: None,
        fix: None,
        binary_path: binary_path.display().to_string(),
        config_path: config_path.display().to_string(),
        pinned_release: PINNED_SD_RELEASE.to_owned(),
        install_type: None,
        release: None,
        platform: None,
        installed_at: None,
        record_error: None,
        version_line: None,
        commit: None,
        pin: pin_state_at(
            binary_path,
            recorded_install(config_path).as_ref(),
            checkout,
            selector,
        ),
    };

    // The record is read even with no binary: provenance is what someone
    // looking at a half-removed install needs.
    if config_path.exists() {
        match InstallRecord::load(config_path) {
            Ok(InstallRecord::Built(build)) => {
                status.install_type = Some("source".to_owned());
                status.release = Some(build.version);
                status.platform = Some(build.acceleration);
                status.installed_at = Some(build.build_date.to_rfc3339());
            }
            Ok(InstallRecord::Prebuilt(record)) => {
                status.install_type = Some(record.install_type);
                status.release = Some(record.version);
                status.platform = Some(record.platform);
                status.installed_at = Some(record.installed_at);
            }
            Err(e) => status.record_error = Some(e.to_string()),
        }
    }

    // One `--version` run is both the check and the version line.
    if status.installed {
        match validate_engine_at(RuntimeKind::StableDiffusion, binary_path) {
            Ok(output) => {
                status.version_line = first_line(&output.stdout);
                status.commit = status.version_line.as_deref().and_then(commit_of);
            }
            Err(not_working) => {
                status.fix = not_working.fix();
                status.health_error = Some(not_working.to_string());
            }
        }
    }
    status
}

/// The commit a `… version <v>, commit <c>` line names.
pub(crate) fn commit_of(line: &str) -> Option<String> {
    let commit = line.rsplit_once(", commit ")?.1.trim();
    (!commit.is_empty()).then(|| commit.to_owned())
}
