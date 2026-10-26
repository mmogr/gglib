//! What an install records about itself: one file per product
//! (`llama-config.json`, `sd-config.json`), in one of two shapes.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::pin::Recorded;

/// What a product's record file holds.
///
/// Two shapes have been written to it, by the two ways of installing, and
/// both are read here. Neither names itself, so the keys decide: a file with
/// a source build's five keys is a [`BuildRecord`], and one with a
/// download's four is a [`PrebuiltRecord`]. Each install writes its own
/// shape over whatever was there; nothing else writes the file, and reading
/// it never does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum InstallRecord {
    /// Built from source on this machine.
    Built(BuildRecord),
    /// Downloaded as a pre-built release.
    Prebuilt(PrebuiltRecord),
}

impl InstallRecord {
    /// Save the record to `path`, replacing what is there.
    pub(crate) fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path, json).context("Failed to write config file")?;
        Ok(())
    }

    /// Load the record at `path`, whichever install wrote it.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path).context("Failed to read config file")?;
        serde_json::from_str(&json).context("Failed to parse config file")
    }

    /// The release it names: a tag, or a source build's short commit.
    pub(crate) fn release(&self) -> &str {
        match self {
            Self::Built(build) => &build.version,
            Self::Prebuilt(record) => &record.version,
        }
    }
}

/// What a source build records about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecord {
    /// The release tag that was checked out (`b11429`), or the short commit
    /// when the build followed upstream's newest or predates builds of a
    /// release.
    pub version: String,
    /// The full commit the checkout was at.
    pub commit_sha: String,
    /// When it was built.
    pub build_date: DateTime<Utc>,
    /// The acceleration compiled in (`Metal`).
    pub acceleration: String,
    /// The `CMake` flags the configure step was given for that acceleration.
    pub cmake_flags: Vec<String>,
}

impl BuildRecord {
    /// The record of a build made now.
    pub(crate) fn new(
        version: String,
        commit_sha: String,
        acceleration: &str,
        cmake_flags: Vec<String>,
    ) -> Self {
        Self {
            version,
            commit_sha,
            build_date: Utc::now(),
            acceleration: acceleration.to_owned(),
            cmake_flags,
        }
    }
}

/// What a pre-built download records about itself: the same four keys for
/// every product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrebuiltRecord {
    /// The release tag that was installed (e.g. `b10327`).
    pub(crate) version: String,
    /// The platform build that was chosen (e.g. `macOS ARM64 (Metal)`).
    pub(crate) platform: String,
    /// Always `prebuilt`.
    pub(crate) install_type: String,
    /// When it was installed, as RFC 3339.
    pub(crate) installed_at: String,
}

impl PrebuiltRecord {
    /// The record of a download made now.
    pub(crate) fn new(version: &str, platform: &str) -> Self {
        Self {
            version: version.to_string(),
            platform: platform.to_string(),
            install_type: "prebuilt".to_string(),
            installed_at: Utc::now().to_rfc3339(),
        }
    }
}

/// What the record at `path` says was installed, for comparing with the
/// pin; `None` with no record or one that cannot be read.
pub(crate) fn recorded_install(path: &Path) -> Option<Recorded> {
    Some(match InstallRecord::load(path).ok()? {
        InstallRecord::Built(build) => Recorded {
            release: build.version,
            commit_sha: Some(build.commit_sha),
        },
        InstallRecord::Prebuilt(record) => Recorded {
            release: record.version,
            commit_sha: None,
        },
    })
}

/// The source build recorded at `path`, when one is.
///
/// `None` is no file, or a pre-built download's record: neither is a build
/// to report. A file that is there and cannot be read is an error.
pub(crate) fn recorded_build(path: &Path) -> Result<Option<BuildRecord>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(match InstallRecord::load(path)? {
        InstallRecord::Built(build) => Some(build),
        InstallRecord::Prebuilt(_) => None,
    })
}

/// The acceleration a source build recorded at `path`.
///
/// `None` also when the file cannot be read: the caller is a launch naming
/// its backend, and a line left out is the honest outcome there.
#[must_use]
pub(crate) fn recorded_acceleration(path: &Path) -> Option<String> {
    Some(recorded_build(path).ok()??.acceleration)
}

/// The release the record at `path` names, when it can be read: what a
/// launch narrates as its runtime.
pub(crate) fn recorded_release(path: &Path) -> Option<String> {
    InstallRecord::load(path)
        .ok()
        .map(|record| record.release().to_owned())
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
