//! The record of how the installed llama.cpp got there: `llama-config.json`.

use super::detect::Acceleration;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A download's record is every product's; it lives with the installer.
pub(super) use crate::binary_install::PrebuiltRecord;

/// What `llama-config.json` holds.
///
/// Two shapes have been written to that file, by the two ways of installing,
/// and both are read here. Neither names itself, so the keys decide: a file
/// with a source build's five keys is a [`BuildConfig`], and one with a
/// download's four is a [`PrebuiltRecord`]. Each install writes its own shape
/// over whatever was there; nothing else writes the file, and reading it
/// never does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum InstallRecord {
    /// llama.cpp was built from source on this machine.
    Built(BuildConfig),
    /// llama.cpp was downloaded as a pre-built release.
    Prebuilt(PrebuiltRecord),
}

impl InstallRecord {
    /// Save the record to `path`, replacing what is there.
    pub(super) fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path, json).context("Failed to write config file")?;
        Ok(())
    }

    /// Load the record at `path`, whichever install wrote it.
    pub(super) fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path).context("Failed to read config file")?;
        let record = serde_json::from_str(&json).context("Failed to parse config file")?;
        Ok(record)
    }

    /// The source build's record, when that is how llama.cpp was installed.
    pub(super) fn into_build(self) -> Option<BuildConfig> {
        match self {
            Self::Built(config) => Some(config),
            Self::Prebuilt(_) => None,
        }
    }
}

/// The source build recorded at `path`, when one is.
///
/// `None` is no file, or a pre-built download's record: neither is a build
/// to report. A file that is there and cannot be read is an error.
pub(super) fn recorded_build(path: &Path) -> Result<Option<BuildConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(InstallRecord::load(path)?.into_build())
}

/// The acceleration the installed llama.cpp was built for, when a source
/// build recorded one at `path`.
///
/// `None` also when the file cannot be read: the caller is a launch naming
/// its backend, and a line left out is the honest outcome there.
#[must_use]
pub(crate) fn recorded_acceleration(path: &Path) -> Option<String> {
    Some(recorded_build(path).ok()??.acceleration)
}

/// Build configuration for llama.cpp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// llama.cpp version/commit short hash
    pub version: String,
    /// Full commit SHA
    pub commit_sha: String,
    /// When the build was created
    pub build_date: DateTime<Utc>,
    /// Acceleration type used
    pub acceleration: String,
    /// `CMake` flags used
    pub cmake_flags: Vec<String>,
}

impl BuildConfig {
    /// Create a new build configuration
    pub fn new(version: String, commit_sha: String, acceleration: Acceleration) -> Self {
        Self {
            version,
            commit_sha,
            build_date: Utc::now(),
            acceleration: acceleration.display_name().to_string(),
            cmake_flags: acceleration
                .cmake_flags()
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
