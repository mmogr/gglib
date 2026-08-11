//! What is installed, as data.
//!
//! `gglib config llama status` used to compute and print in one pass, which
//! left the GUI with only `llama_installed: bool` from the setup status. This
//! module answers the same question as a value; [`super::handle_status`] is
//! now a printer over it and the Axum `system/llama-status` route serialises
//! it directly, so the two surfaces cannot drift.

use super::config::BuildConfig;
use gglib_core::domain::RuntimeCapabilities;
use gglib_core::paths::{llama_config_path, llama_server_path};
use serde::Serialize;

/// The recorded build, when one exists.
///
/// Absent for a prebuilt download: that path never writes a `BuildConfig`.
/// An installed binary with no build record is normal, not an error.
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

/// What the binary reports about itself when probed.
///
/// A projection of [`RuntimeCapabilities`] rather than that type itself:
/// `RuntimeCapabilities` is a shared core type that is also `Deserialize`d
/// from stored records, so it carries no `rename_all` and would put
/// snake_case keys inside this otherwise-camelCase payload.
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
    /// Set when a build record exists but could not be read.
    pub build_error: Option<String>,
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
    let binary_path = llama_server_path().map_err(|e| anyhow::anyhow!("{}", e))?;
    let config_path = llama_config_path().map_err(|e| anyhow::anyhow!("{}", e))?;

    let mut status = LlamaStatus {
        installed: binary_path.exists(),
        binary_path: binary_path.display().to_string(),
        config_path: config_path.display().to_string(),
        healthy: false,
        health_error: None,
        build: None,
        build_error: None,
        runtime: None,
    };

    if !status.installed {
        return Ok(status);
    }

    // Read the build record BEFORE validating. Reading a JSON file is safe on
    // a broken binary, and provenance is exactly what someone debugging an
    // unhealthy install needs — reporting "no build record" merely because we
    // returned early would assert the install came from somewhere it did not.
    if config_path.exists() {
        match BuildConfig::load(&config_path) {
            Ok(config) => status.build = Some(config.into()),
            Err(e) => status.build_error = Some(e.to_string()),
        }
    }

    match super::validate_llama_binary(&binary_path) {
        Ok(()) => status.healthy = true,
        Err(e) => {
            status.health_error = Some(e.to_string());
            // A binary that fails validation cannot be trusted to answer
            // `--version` sensibly, so stop before probing it.
            return Ok(status);
        }
    }

    // Read through the probe rather than a local `--version` call so this
    // surface and the launch banner cannot report different runtimes for the
    // same binary.
    status.runtime = Some((&super::runtime_probe::probe(&binary_path)).into());

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GUI's `LlamaStatus` in `types/setup.ts` is hand-mirrored, so the
    /// casing is a contract rather than an implementation detail.
    #[test]
    fn status_serialises_as_camel_case() {
        let status = LlamaStatus {
            installed: true,
            binary_path: "/tmp/llama-server".into(),
            config_path: "/tmp/llama-config.json".into(),
            healthy: false,
            health_error: Some("not executable".into()),
            build: None,
            build_error: None,
            runtime: None,
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["binaryPath"], "/tmp/llama-server");
        assert_eq!(json["configPath"], "/tmp/llama-config.json");
        assert_eq!(json["healthError"], "not executable");
        assert!(json.get("binary_path").is_none());
    }

    /// The runtime block is a projection, not the core type — serialising
    /// `RuntimeCapabilities` directly would put snake_case keys inside this
    /// camelCase payload, which is what the TS mirror silently tripped over.
    #[test]
    fn runtime_block_is_camel_case() {
        let caps = gglib_core::domain::RuntimeCapabilities {
            build: Some(9656),
            commit: None,
            version_line: "version: 9656 (deadbee)".into(),
            flags: Default::default(),
        };

        let json = serde_json::to_value(LlamaRuntimeInfo::from(&caps)).unwrap();
        assert_eq!(json["versionLine"], "version: 9656 (deadbee)");
        assert_eq!(json["build"], 9656);
        assert!(json.get("version_line").is_none());
        // Rendered, not structural — no consumer branches on flag names.
        assert!(json["flags"].is_string());
    }

    #[test]
    fn build_info_carries_both_notions_of_version() {
        let info = LlamaBuildInfo::from(BuildConfig {
            version: "abc1234".into(),
            commit_sha: "abc1234def".into(),
            build_date: chrono::Utc::now(),
            acceleration: "Metal".into(),
            cmake_flags: vec!["-DGGML_METAL=ON".into()],
        });

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["version"], "abc1234");
        assert_eq!(json["commitSha"], "abc1234def");
        assert_eq!(json["cmakeFlags"][0], "-DGGML_METAL=ON");
        // RFC 3339 rather than a locale-formatted string: the GUI parses it.
        assert!(json["buildDate"].as_str().unwrap().contains('T'));
    }
}
