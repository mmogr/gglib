//! Build configuration storage and management.

#[cfg(feature = "cli")]
use super::detect::Acceleration;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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
    #[cfg(feature = "cli")]
    pub fn new(version: String, commit_sha: String, acceleration: Acceleration) -> Self {
        Self {
            version,
            commit_sha,
            build_date: Utc::now(),
            acceleration: acceleration.display_name().to_string(),
            cmake_flags: acceleration
                .cmake_flags()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    /// Save configuration to file
    #[cfg(feature = "cli")]
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(path, json).context("Failed to write config file")?;
        Ok(())
    }

    /// Load configuration from file
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path).context("Failed to read config file")?;
        let config = serde_json::from_str(&json).context("Failed to parse config file")?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "cli")]
    use super::*;
    #[cfg(feature = "cli")]
    use tempfile::tempdir;

    #[test]
    #[cfg(feature = "cli")]
    fn test_build_config_roundtrip() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test-config.json");

        let original = BuildConfig::new(
            "b1234".to_string(),
            "abc123def456".to_string(),
            Acceleration::Metal,
        );

        original.save(&config_path).unwrap();
        let loaded = BuildConfig::load(&config_path).unwrap();

        assert_eq!(original.version, loaded.version);
        assert_eq!(original.commit_sha, loaded.commit_sha);
        assert_eq!(original.acceleration, loaded.acceleration);
        assert_eq!(original.cmake_flags, loaded.cmake_flags);
    }
}
