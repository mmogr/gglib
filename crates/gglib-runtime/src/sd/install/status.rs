//! What is installed for image generation, as data.

use gglib_core::paths::{sd_config_path, sd_server_path};
use gglib_core::utils::process::cmd;
use serde::Serialize;
use std::path::Path;

use super::record::SdInstallRecord;
use super::release::PINNED_SD_RELEASE;

/// Everything a status report says about the installed `sd-server`.
///
/// Two versions are reported and kept apart, as llama.cpp's status does:
/// [`Self::release`] is what the install recorded, [`Self::version_line`] is
/// what the binary says when asked. They disagree for a binary put there by
/// hand, which is what someone debugging a launch needs to see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SdStatus {
    /// Whether the binary is there.
    pub installed: bool,
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
}

/// Inspect the installed `sd-server`, if any. Degraded states are values;
/// only a data directory that cannot be resolved is an `Err`.
pub fn sd_status() -> anyhow::Result<SdStatus> {
    Ok(sd_status_at(&sd_server_path()?, &sd_config_path()?))
}

/// [`sd_status`] for a binary and a record at the paths given.
pub(crate) fn sd_status_at(binary_path: &Path, config_path: &Path) -> SdStatus {
    let mut status = SdStatus {
        installed: binary_path.is_file(),
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
    };

    // The record is read even with no binary: provenance is what someone
    // looking at a half-removed install needs.
    if config_path.exists() {
        match SdInstallRecord::load(config_path) {
            Ok(SdInstallRecord::Built(build)) => {
                status.install_type = Some("source".to_owned());
                status.release = Some(build.version);
                status.platform = Some(build.acceleration);
                status.installed_at = Some(build.build_date.to_rfc3339());
            }
            Ok(SdInstallRecord::Prebuilt(record)) => {
                status.install_type = Some(record.install_type);
                status.release = Some(record.version);
                status.platform = Some(record.platform);
                status.installed_at = Some(record.installed_at);
            }
            Err(e) => status.record_error = Some(e.to_string()),
        }
    }

    if status.installed {
        status.version_line = version_line(binary_path);
        status.commit = status.version_line.as_deref().and_then(commit_of);
    }
    status
}

/// The first line `binary --version` prints, when it runs and prints one.
fn version_line(binary: &Path) -> Option<String> {
    let output = cmd(binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// The commit a `… version <v>, commit <c>` line names.
pub(crate) fn commit_of(line: &str) -> Option<String> {
    let commit = line.rsplit_once(", commit ")?.1.trim();
    (!commit.is_empty()).then(|| commit.to_owned())
}
