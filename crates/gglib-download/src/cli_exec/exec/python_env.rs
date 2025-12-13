//! Python environment setup for the fast downloader.
//!
//! Manages Python venv creation, requirements installation, and helper script deployment.
//! Sync module with clear error types — caller wraps for async orchestration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use gglib_core::paths::data_root;
use thiserror::Error;
use tokio::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// ============================================================================
// Constants
// ============================================================================

const PY_HELPER_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/hf_xet_downloader.py"
));

const ENV_MARKER_NAME: &str = ".gglib-hf-xet.json";

const PY_REQUIREMENTS: &[&str] = &["huggingface_hub>=1.1.5", "hf_xet>=0.6.0"];

#[cfg(target_os = "windows")]
const PYTHON_CANDIDATES: &[&str] = &["python"];

#[cfg(not(target_os = "windows"))]
const PYTHON_CANDIDATES: &[&str] = &["python3", "python"];

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during Python environment setup.
#[derive(Error, Debug)]
pub enum EnvSetupError {
    #[error("Python not found in PATH (tried: {0})")]
    PythonNotFound(String),

    #[error("Failed to create virtualenv at {path}: {reason}")]
    CreateEnvFailed { path: PathBuf, reason: String },

    #[error("Failed to install requirements: {0}")]
    RequirementsFailed(String),

    #[error("Failed to write helper script at {path}: {reason}")]
    ScriptWriteFailed { path: PathBuf, reason: String },

    #[error("Failed to create directory {path}: {reason}")]
    DirectoryCreateFailed { path: PathBuf, reason: String },

    #[error("Failed to determine data root: {0}")]
    DataRootFailed(String),

    #[error("Marker file error: {0}")]
    MarkerError(String),
}

// ============================================================================
// Environment Marker
// ============================================================================

use serde::{Deserialize, Serialize};

/// Marker file to track environment freshness.
#[derive(Deserialize, Serialize)]
struct EnvMarker {
    helper_version: String,
    requirements: Vec<String>,
}

impl EnvMarker {
    fn current() -> Self {
        Self {
            helper_version: env!("CARGO_PKG_VERSION").to_string(),
            requirements: PY_REQUIREMENTS.iter().copied().map(String::from).collect(),
        }
    }

    fn matches(&self) -> bool {
        self.helper_version == env!("CARGO_PKG_VERSION")
            && self.requirements
                == PY_REQUIREMENTS
                    .iter()
                    .copied()
                    .map(String::from)
                    .collect::<Vec<_>>()
    }
}

// ============================================================================
// Python Environment
// ============================================================================

/// A prepared Python environment for running the fast downloader.
///
/// Use `PythonEnvironment::prepare()` to create and validate the environment.
/// The environment includes:
/// - A dedicated virtualenv with required packages
/// - The helper script deployed to a known location
pub struct PythonEnvironment {
    env_dir: PathBuf,
    script_path: PathBuf,
}

impl PythonEnvironment {
    /// Prepare the Python environment, creating it if necessary.
    ///
    /// This will:
    /// 1. Find a suitable Python interpreter
    /// 2. Create a virtualenv if it doesn't exist
    /// 3. Install/update requirements if the marker is stale
    /// 4. Deploy the helper script
    ///
    /// Returns `Err` if Python is not found or setup fails.
    pub async fn prepare() -> Result<Self, EnvSetupError> {
        let env_dir = get_env_directory()?;
        let script_path = get_script_path()?;

        // Ensure parent directories exist
        ensure_parent_dir(&env_dir)?;
        ensure_parent_dir(&script_path)?;

        let env = Self {
            env_dir,
            script_path,
        };

        env.write_script()?;
        env.ensure_env_ready().await?;

        Ok(env)
    }

    /// Get the path to the Python interpreter in this environment.
    pub fn python_path(&self) -> PathBuf {
        if cfg!(windows) {
            self.env_dir.join("Scripts").join("python.exe")
        } else {
            let bin = self.env_dir.join("bin");
            let python3 = bin.join("python3");
            if python3.exists() {
                python3
            } else {
                bin.join("python")
            }
        }
    }

    /// Get the path to the helper script.
    pub fn script_path(&self) -> &Path {
        &self.script_path
    }

    // ------------------------------------------------------------------------
    // Internal methods
    // ------------------------------------------------------------------------

    async fn ensure_env_ready(&self) -> Result<(), EnvSetupError> {
        if !self.python_path().exists() {
            self.create_env().await?;
        }

        if !self.marker_is_fresh()? {
            self.install_requirements().await?;
            self.write_marker()?;
        }

        Ok(())
    }

    async fn create_env(&self) -> Result<(), EnvSetupError> {
        let bootstrap = find_bootstrap_python()?;

        println!(
            "ℹ️  Creating Python environment for fast downloads at {}...",
            self.env_dir.display()
        );

        let status = Command::new(&bootstrap)
            .arg("-m")
            .arg("venv")
            .arg(&self.env_dir)
            .status()
            .await
            .map_err(|e| EnvSetupError::CreateEnvFailed {
                path: self.env_dir.clone(),
                reason: e.to_string(),
            })?;

        if !status.success() {
            return Err(EnvSetupError::CreateEnvFailed {
                path: self.env_dir.clone(),
                reason: format!("python -m venv exited with {status}"),
            });
        }

        Ok(())
    }

    async fn install_requirements(&self) -> Result<(), EnvSetupError> {
        println!("ℹ️  Installing fast download dependencies...");

        let python = self.python_path();

        // Upgrade pip first
        run_python_command(&python, &["-m", "pip", "install", "--upgrade", "pip"]).await?;

        // Install requirements
        let mut args = vec!["-m", "pip", "install", "--upgrade"];
        args.extend(PY_REQUIREMENTS);
        run_python_command(&python, &args).await?;

        Ok(())
    }

    fn write_script(&self) -> Result<(), EnvSetupError> {
        if let Some(parent) = self.script_path.parent() {
            fs::create_dir_all(parent).map_err(|e| EnvSetupError::DirectoryCreateFailed {
                path: parent.to_path_buf(),
                reason: e.to_string(),
            })?;
        }

        fs::write(&self.script_path, PY_HELPER_SOURCE).map_err(|e| {
            EnvSetupError::ScriptWriteFailed {
                path: self.script_path.clone(),
                reason: e.to_string(),
            }
        })?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&self.script_path)
                .map_err(|e| EnvSetupError::ScriptWriteFailed {
                    path: self.script_path.clone(),
                    reason: e.to_string(),
                })?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&self.script_path, perms).map_err(|e| {
                EnvSetupError::ScriptWriteFailed {
                    path: self.script_path.clone(),
                    reason: e.to_string(),
                }
            })?;
        }

        Ok(())
    }

    fn marker_is_fresh(&self) -> Result<bool, EnvSetupError> {
        let marker_path = self.env_dir.join(ENV_MARKER_NAME);

        if !marker_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(&marker_path)
            .map_err(|e| EnvSetupError::MarkerError(e.to_string()))?;

        let marker: EnvMarker = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => return Ok(false),
        };

        Ok(marker.matches())
    }

    fn write_marker(&self) -> Result<(), EnvSetupError> {
        let marker = EnvMarker::current();
        let marker_path = self.env_dir.join(ENV_MARKER_NAME);

        let content = serde_json::to_string_pretty(&marker)
            .map_err(|e| EnvSetupError::MarkerError(e.to_string()))?;

        fs::write(&marker_path, content)
            .map_err(|e| EnvSetupError::MarkerError(format!("Failed to write marker: {e}")))?;

        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find a Python interpreter suitable for bootstrapping the venv.
fn find_bootstrap_python() -> Result<PathBuf, EnvSetupError> {
    for candidate in PYTHON_CANDIDATES {
        if let Ok(path) = which::which(candidate) {
            return Ok(path);
        }
    }

    Err(EnvSetupError::PythonNotFound(PYTHON_CANDIDATES.join(", ")))
}

/// Run a Python command and check for success.
async fn run_python_command(python: &Path, args: &[&str]) -> Result<(), EnvSetupError> {
    let status = Command::new(python)
        .args(args)
        .status()
        .await
        .map_err(|e| EnvSetupError::RequirementsFailed(e.to_string()))?;

    if !status.success() {
        return Err(EnvSetupError::RequirementsFailed(format!(
            "{} {args:?} exited with {status}",
            python.display()
        )));
    }

    Ok(())
}

/// Get the directory for the Python environment.
fn get_env_directory() -> Result<PathBuf, EnvSetupError> {
    let root = data_root().map_err(|e| EnvSetupError::DataRootFailed(e.to_string()))?;
    Ok(root.join(".conda").join("gglib-hf-xet"))
}

/// Get the path for the helper script.
fn get_script_path() -> Result<PathBuf, EnvSetupError> {
    let root = data_root().map_err(|e| EnvSetupError::DataRootFailed(e.to_string()))?;
    Ok(root
        .join(".gglib-runtime")
        .join("python")
        .join("hf_xet_downloader.py"))
}

/// Ensure a path's parent directory exists.
fn ensure_parent_dir(path: &Path) -> Result<(), EnvSetupError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| EnvSetupError::DirectoryCreateFailed {
            path: parent.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_marker_current_matches() {
        let marker = EnvMarker::current();
        assert!(marker.matches());
    }

    #[test]
    fn test_env_marker_version_mismatch() {
        let marker = EnvMarker {
            helper_version: "0.0.0".to_string(),
            requirements: PY_REQUIREMENTS.iter().copied().map(String::from).collect(),
        };
        assert!(!marker.matches());
    }

    #[test]
    fn test_env_marker_requirements_mismatch() {
        let marker = EnvMarker {
            helper_version: env!("CARGO_PKG_VERSION").to_string(),
            requirements: vec!["different>=1.0.0".to_string()],
        };
        assert!(!marker.matches());
    }

    #[test]
    fn test_python_not_found_error_display() {
        let err = EnvSetupError::PythonNotFound("python3, python".to_string());
        assert!(err.to_string().contains("Python not found"));
    }

    #[test]
    fn test_create_env_failed_error_display() {
        let err = EnvSetupError::CreateEnvFailed {
            path: PathBuf::from("/tmp/env"),
            reason: "permission denied".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("virtualenv"));
        assert!(msg.contains("permission denied"));
    }
}
