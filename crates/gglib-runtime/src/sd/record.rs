//! The record of how the installed `sd-server` got there: `sd-config.json`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::binary_install::PrebuiltRecord;
use crate::llama::Acceleration;

/// What `sd-config.json` holds: one of two shapes, told apart by their keys,
/// as `llama-config.json` is. A source build's five keys are an
/// [`SdBuildRecord`]; a download's four are a [`PrebuiltRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum SdInstallRecord {
    /// Built from source on this machine.
    Built(SdBuildRecord),
    /// Downloaded as a pre-built release.
    Prebuilt(PrebuiltRecord),
}

impl SdInstallRecord {
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

    /// The release it names: a tag, or a short commit.
    pub(crate) fn release(&self) -> &str {
        match self {
            Self::Built(build) => &build.version,
            Self::Prebuilt(record) => &record.version,
        }
    }
}

/// The release the installed `sd-server`'s record names, when there is a
/// readable record: what a launch narrates as its runtime.
pub(crate) fn recorded_release() -> Option<String> {
    let path = gglib_core::paths::sd_config_path().ok()?;
    SdInstallRecord::load(&path)
        .ok()
        .map(|record| record.release().to_owned())
}

/// What a source build records about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SdBuildRecord {
    /// The release tag that was checked out (`master-948-228c707`), or the
    /// short commit when the build followed upstream's newest.
    pub(crate) version: String,
    /// The full commit the checkout was at.
    pub(crate) commit_sha: String,
    /// When it was built.
    pub(crate) build_date: DateTime<Utc>,
    /// The acceleration compiled in (`Metal`).
    pub(crate) acceleration: String,
    /// The `CMake` flags the configure step was given beyond the defaults.
    pub(crate) cmake_flags: Vec<String>,
}

impl SdBuildRecord {
    /// The record of a build made now.
    pub(crate) fn new(
        version: String,
        commit_sha: String,
        acceleration: Acceleration,
        cmake_flags: Vec<String>,
    ) -> Self {
        Self {
            version,
            commit_sha,
            build_date: Utc::now(),
            acceleration: acceleration.display_name().to_string(),
            cmake_flags,
        }
    }
}
