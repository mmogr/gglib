//! What is installed, as data.
//!
//! This module answers "what is installed" as a value:
//! [`super::handle_status`] prints it for `gglib config llama status`, and the
//! Axum `system/llama-status` route serialises it directly, so the two
//! surfaces cannot drift.

use super::config::{BuildConfig, InstallRecord, PrebuiltRecord};
use gglib_core::domain::RuntimeCapabilities;
use gglib_core::paths::{llama_config_path, llama_server_path};
use serde::Serialize;
use std::path::Path;

/// The recorded build, when one exists.
///
/// Absent for a pre-built download, which records a [`LlamaPrebuiltInfo`]
/// in its place. An installed binary with no build record is normal, not an
/// error.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaBuildInfo {
    /// Short git hash recorded when the binary was built.
    pub version: String,
    pub commit_sha: String,
    pub build_date: String,
    pub acceleration: String,
    pub cmake_flags: Vec<String>,
}

impl From<BuildConfig> for LlamaBuildInfo {
    fn from(config: BuildConfig) -> Self {
        Self {
            version: config.version,
            commit_sha: config.commit_sha,
            build_date: config.build_date.to_rfc3339(),
            acceleration: config.acceleration,
            cmake_flags: config.cmake_flags,
        }
    }
}

/// The recorded pre-built download, when that is how llama.cpp was installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaPrebuiltInfo {
    /// The llama.cpp release tag that was downloaded.
    pub version: String,
    /// The platform build that was chosen.
    pub platform: String,
    pub installed_at: String,
}

impl From<PrebuiltRecord> for LlamaPrebuiltInfo {
    fn from(record: PrebuiltRecord) -> Self {
        Self {
            version: record.version,
            platform: record.platform,
            installed_at: record.installed_at,
        }
    }
}

/// What the binary reports about itself when probed.
///
/// A projection of [`RuntimeCapabilities`] rather than that type itself:
/// `RuntimeCapabilities` is a shared core type that is also `Deserialize`d
/// from stored records, so it carries no `rename_all` and would put
/// `snake_case` keys inside this otherwise-camelCase payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaRuntimeInfo {
    /// llama.cpp build number, absent when the binary could not be identified.
    pub build: Option<u32>,
    pub commit: Option<String>,
    pub version_line: String,
    /// Native capability flags, already rendered for display. A string rather
    /// than the bitflags themselves so their variant names do not become wire
    /// API — no consumer branches on them, both surfaces only print them.
    pub flags: String,
}

impl From<&RuntimeCapabilities> for LlamaRuntimeInfo {
    fn from(caps: &RuntimeCapabilities) -> Self {
        Self {
            build: caps.build,
            commit: caps.commit.clone(),
            version_line: caps.version_line.clone(),
            flags: format!("{:?}", caps.flags),
        }
    }
}

/// Everything `gglib config llama status` reports.
///
/// Two notions of "version" coexist here deliberately and must not be merged:
/// [`LlamaBuildInfo::version`] is what gglib recorded when it built the
/// binary, while [`Self::runtime`] is what the binary reports about itself
/// when probed. They disagree for a hand-installed or prebuilt binary, and
/// that disagreement is exactly what a user debugging a launch needs to see.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlamaStatus {
    pub installed: bool,
    pub binary_path: String,
    pub config_path: String,
    /// Whether the binary passed validation. False whenever `healthError` is set.
    pub healthy: bool,
    /// Why validation failed, including its remediation text.
    pub health_error: Option<String>,
    pub build: Option<LlamaBuildInfo>,
    /// Set when a record of the install exists but could not be read.
    pub build_error: Option<String>,
    /// The download's record, for an install that was not built here.
    pub prebuilt: Option<LlamaPrebuiltInfo>,
    /// What the binary says it is. Absent when nothing is installed to probe;
    /// present-but-unidentified when the probe could not parse a build number.
    pub runtime: Option<LlamaRuntimeInfo>,
}

/// Inspect the installed llama.cpp, if any.
///
/// Degraded states are values, not errors: not installed, installed but
/// failing validation, and installed without a build record are all reported
/// in the returned struct. Only a failure to resolve the data directory
/// itself is an `Err`.
pub fn llama_status() -> anyhow::Result<LlamaStatus> {
    Ok(status_at(&llama_server_path()?, &llama_config_path()?))
}

/// [`llama_status`] for a binary and a record at the paths given.
fn status_at(binary_path: &Path, config_path: &Path) -> LlamaStatus {
    let mut status = LlamaStatus {
        installed: binary_path.exists(),
        binary_path: binary_path.display().to_string(),
        config_path: config_path.display().to_string(),
        healthy: false,
        health_error: None,
        build: None,
        build_error: None,
        prebuilt: None,
        runtime: None,
    };

    if !status.installed {
        return status;
    }

    // Read the install's record BEFORE validating. Reading a JSON file is safe
    // on a broken binary, and provenance is exactly what someone debugging an
    // unhealthy install needs — reporting "no build record" merely because we
    // returned early would assert the install came from somewhere it did not.
    if config_path.exists() {
        match InstallRecord::load(config_path) {
            Ok(InstallRecord::Built(config)) => status.build = Some(config.into()),
            Ok(InstallRecord::Prebuilt(record)) => status.prebuilt = Some(record.into()),
            Err(e) => status.build_error = Some(e.to_string()),
        }
    }

    match super::validate_llama_binary(binary_path) {
        Ok(()) => status.healthy = true,
        Err(e) => {
            status.health_error = Some(e.to_string());
            // A binary that fails validation cannot be trusted to answer
            // `--version` sensibly, so stop before probing it.
            return status;
        }
    }

    // Read through the probe rather than a local `--version` call so this
    // surface and the launch banner cannot report different runtimes for the
    // same binary.
    status.runtime = Some((&super::runtime_probe::probe(binary_path)).into());

    status
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
