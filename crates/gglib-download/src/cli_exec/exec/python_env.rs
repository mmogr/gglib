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

const PYTHON_OVERRIDE_ENV: &str = "GGLIB_PYTHON";

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

    #[error("Python interpreter validation failed at {path}: {reason}")]
    PythonInvalid { path: PathBuf, reason: String },

    #[error("No working Python interpreter found (tried: {tried}). Last error: {last_error}")]
    PythonValidationFailed { tried: String, last_error: String },

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

        // Validate the interpreter we will actually run.
        // This catches environment pollution (e.g., PYTHONHOME/PYTHONPATH) early.
        validate_python_interpreter(&env.python_path()).await?;

        Ok(env)
    }

    /// Preflight check for fast downloads.
    ///
    /// This is intentionally lightweight: it validates that a bootstrap Python
    /// interpreter can import the standard library (including `encodings`).
    ///
    /// Returns the resolved interpreter path string (as reported by Python).
    pub async fn preflight() -> Result<String, EnvSetupError> {
        let bootstrap = find_bootstrap_python_validated().await?;
        validate_python_interpreter(&bootstrap).await
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
        let bootstrap = find_bootstrap_python_validated().await?;

        println!(
            "ℹ️  Creating Python environment for fast downloads at {}...",
            self.env_dir.display()
        );

        let mut cmd = Command::new(&bootstrap);
        apply_python_subprocess_isolation(&mut cmd);
        let status = cmd
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
async fn find_bootstrap_python_validated() -> Result<PathBuf, EnvSetupError> {
    // 1) Explicit override
    if let Some(override_path) = env::var_os(PYTHON_OVERRIDE_ENV).map(PathBuf::from) {
        if !override_path.exists() {
            return Err(EnvSetupError::PythonInvalid {
                path: override_path,
                reason: "path does not exist".to_string(),
            });
        }

        validate_python_interpreter(&override_path).await?;
        return Ok(override_path);
    }

    // 2) PATH discovery (prefer python3 on non-Windows)
    let mut tried: Vec<String> = Vec::new();
    let mut last_error: Option<String> = None;

    for candidate in PYTHON_CANDIDATES {
        tried.push((*candidate).to_string());
        let Ok(path) = which::which(candidate) else {
            continue;
        };

        match validate_python_interpreter(&path).await {
            Ok(_) => return Ok(path),
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }
    }

    if let Some(last_error) = last_error {
        return Err(EnvSetupError::PythonValidationFailed {
            tried: tried.join(", "),
            last_error,
        });
    }

    Err(EnvSetupError::PythonNotFound(PYTHON_CANDIDATES.join(", ")))
}

/// Run a Python command and check for success.
async fn run_python_command(python: &Path, args: &[&str]) -> Result<(), EnvSetupError> {
    let mut cmd = Command::new(python);
    apply_python_subprocess_isolation(&mut cmd);
    cmd.args(args);

    let output = cmd
        .output()
        .await
        .map_err(|e| EnvSetupError::RequirementsFailed(e.to_string()))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let mut details = format!(
            "{} {args:?} exited with {}",
            python.display(),
            output.status
        );
        if !stdout.is_empty() {
            use std::fmt::Write;
            let _ = write!(details, "\nstdout: {stdout}");
        }
        if !stderr.is_empty() {
            use std::fmt::Write;
            let _ = write!(details, "\nstderr: {stderr}");
        }
        return Err(EnvSetupError::RequirementsFailed(details));
    }

    Ok(())
}

/// Apply a denylist-based environment isolation for Python subprocesses.
///
/// This prevents a polluted parent shell (e.g., conda) from breaking the child
/// interpreter with missing stdlib modules like `encodings`.
fn apply_python_subprocess_isolation(cmd: &mut Command) {
    // Explicitly remove common environment variables that can corrupt stdlib resolution.
    for key in [
        "PYTHONHOME",
        "PYTHONPATH",
        "PYTHONUSERBASE",
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
        "CONDA_DEFAULT_ENV",
        "CONDA_PROMPT_MODIFIER",
        "CONDA_SHLVL",
        "CONDA_EXE",
        "CONDA_PYTHON_EXE",
        "_CE_CONDA",
        "_CE_M",
    ] {
        cmd.env_remove(key);
    }

    // Prevent user-site packages from influencing imports.
    cmd.env("PYTHONNOUSERSITE", "1");
}

/// Validate that the given Python interpreter can import the standard library.
///
/// Returns the resolved `sys.executable` string on success.
async fn validate_python_interpreter(python: &Path) -> Result<String, EnvSetupError> {
    let mut cmd = Command::new(python);
    apply_python_subprocess_isolation(&mut cmd);
    cmd.arg("-c")
        .arg("import encodings, sys; print(sys.executable)");

    let output = cmd
        .output()
        .await
        .map_err(|e| EnvSetupError::PythonInvalid {
            path: python.to_path_buf(),
            reason: format!("failed to spawn: {e}"),
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        let mut reason = format!("exited with {}", output.status);
        if !stdout.is_empty() {
            use std::fmt::Write;
            let _ = write!(reason, "\nstdout: {stdout}");
        }
        if !stderr.is_empty() {
            use std::fmt::Write;
            let _ = write!(reason, "\nstderr: {stderr}");
        }

        return Err(EnvSetupError::PythonInvalid {
            path: python.to_path_buf(),
            reason,
        });
    }

    let exe = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if exe.is_empty() {
        python.display().to_string()
    } else {
        exe
    })
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

    /// Test that environment isolation properly removes polluted environment variables
    /// and sets PYTHONNOUSERSITE=1 to prevent stdlib resolution issues.
    ///
    /// This test simulates a dirty environment by setting polluted variables directly
    /// on the Command object, then verifies that `apply_python_subprocess_isolation`
    /// removes them and sets PYTHONNOUSERSITE=1.
    #[tokio::test]
    async fn test_environment_isolation_removes_polluted_vars() {
        // Find a working Python interpreter
        let Ok(python) = which::which("python3").or_else(|_| which::which("python")) else {
            eprintln!("Python not available for test, skipping environment isolation test");
            return;
        };

        // Create a command with a "dirty" environment simulating a conda/virtualenv shell
        let mut cmd = Command::new(python);

        // Simulate polluted environment by setting variables on the Command
        cmd.env("PYTHONHOME", "/fake/python/home")
            .env("PYTHONPATH", "/fake/python/path")
            .env("PYTHONUSERBASE", "/fake/user/base")
            .env("VIRTUAL_ENV", "/fake/venv")
            .env("CONDA_PREFIX", "/fake/conda")
            .env("CONDA_DEFAULT_ENV", "fake_env")
            .env("CONDA_PROMPT_MODIFIER", "(fake_env)")
            .env("CONDA_SHLVL", "1");

        // Apply our isolation function - this should remove the polluted vars
        apply_python_subprocess_isolation(&mut cmd);

        // Use Python to print its environment variables that we care about
        cmd.arg("-c").arg(
            "import os, sys; \
             print('PYTHONHOME=' + os.getenv('PYTHONHOME', 'UNSET')); \
             print('PYTHONPATH=' + os.getenv('PYTHONPATH', 'UNSET')); \
             print('PYTHONUSERBASE=' + os.getenv('PYTHONUSERBASE', 'UNSET')); \
             print('VIRTUAL_ENV=' + os.getenv('VIRTUAL_ENV', 'UNSET')); \
             print('CONDA_PREFIX=' + os.getenv('CONDA_PREFIX', 'UNSET')); \
             print('CONDA_DEFAULT_ENV=' + os.getenv('CONDA_DEFAULT_ENV', 'UNSET')); \
             print('PYTHONNOUSERSITE=' + os.getenv('PYTHONNOUSERSITE', 'UNSET')); \
             print('SUCCESS')",
        );

        let output = cmd.output().await.expect("Failed to run Python subprocess");

        // Verify the Python subprocess ran successfully
        assert!(
            output.status.success(),
            "Python subprocess failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Assert that all polluted variables were removed (should be UNSET)
        assert!(
            stdout.contains("PYTHONHOME=UNSET"),
            "PYTHONHOME should be removed, got: {stdout}"
        );
        assert!(
            stdout.contains("PYTHONPATH=UNSET"),
            "PYTHONPATH should be removed, got: {stdout}"
        );
        assert!(
            stdout.contains("PYTHONUSERBASE=UNSET"),
            "PYTHONUSERBASE should be removed, got: {stdout}"
        );
        assert!(
            stdout.contains("VIRTUAL_ENV=UNSET"),
            "VIRTUAL_ENV should be removed, got: {stdout}"
        );
        assert!(
            stdout.contains("CONDA_PREFIX=UNSET"),
            "CONDA_PREFIX should be removed, got: {stdout}"
        );
        assert!(
            stdout.contains("CONDA_DEFAULT_ENV=UNSET"),
            "CONDA_DEFAULT_ENV should be removed, got: {stdout}"
        );

        // Assert that PYTHONNOUSERSITE was explicitly set to '1'
        assert!(
            stdout.contains("PYTHONNOUSERSITE=1"),
            "PYTHONNOUSERSITE should be set to '1', got: {stdout}"
        );

        // Verify Python ran successfully (can import encodings)
        assert!(
            stdout.contains("SUCCESS"),
            "Python should successfully import stdlib and print SUCCESS"
        );
    }
}
