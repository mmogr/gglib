//! Python environment setup for the fast downloader.
//!
//! Manages Python venv creation, requirements installation, and helper script deployment.
//! Sync module with clear error types — caller wraps for async orchestration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use gglib_core::paths::data_root;
use gglib_core::utils::process::async_cmd;
use thiserror::Error;
use tokio::process::Command;

use super::python_bridge::NoticeCallback;

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

/// Directory name of the managed environment, under its parent.
const ENV_NAME: &str = "gglib-hf-xet";

/// Parent directory of the managed environment, under the data root.
const ENV_PARENT_DIR: &str = ".python";

/// Where the environment used to live.
///
/// It was never a conda environment — [`PythonEnvironment::create_env`] has
/// always built a plain venv — but it shipped under `.conda/`, which reads as a
/// promise the code does not make. Existing installs have several hundred
/// megabytes sitting at the old path, so [`resolve_env_directory`] keeps using
/// it when it is there rather than silently rebuilding under the new name.
const LEGACY_ENV_PARENT_DIR: &str = ".conda";

const PY_REQUIREMENTS: &[&str] = &["huggingface_hub>=1.1.5", "hf_xet>=0.6.0"];

/// Interpreter names looked up on `PATH`, in preference order.
///
/// The versioned names matter more than they look: pyenv and asdf install
/// shims named `python3.12`, and several distros ship no unversioned `python3`
/// at all. Trying only `python3`/`python` is why a machine with a perfectly
/// good interpreter could not enable the accelerator.
#[cfg(target_os = "windows")]
const PYTHON_CANDIDATES: &[&str] = &[
    "python",
    "python3",
    "python3.13",
    "python3.12",
    "python3.11",
    "python3.10",
    "python3.9",
];

#[cfg(not(target_os = "windows"))]
const PYTHON_CANDIDATES: &[&str] = &[
    "python3",
    "python",
    "python3.13",
    "python3.12",
    "python3.11",
    "python3.10",
    "python3.9",
];

/// Oldest interpreter the requirements will install against.
///
/// Checked at discovery rather than left to fail during the install: a user
/// with a stale `python3` on `PATH` and a current one under pyenv should get
/// the current one, not an error about a wheel that has no build for 3.8.
const MIN_PYTHON: (u32, u32) = (3, 9);

/// Conda-family install prefixes, relative to the home directory.
const CONDA_HOME_PREFIXES: &[&str] = &["miniforge3", "mambaforge", "miniconda3", "anaconda3"];

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

    #[error("Python at {path} is {found}, but {}.{} or newer is required", required.0, required.1)]
    PythonTooOld {
        path: PathBuf,
        found: String,
        required: (u32, u32),
    },

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
    /// `notice` receives transient setup notes ("creating environment...",
    /// "installing dependencies...") for display on a per-download bar. When
    /// `None` (the preflight/no-callback paths), those notes fall back to
    /// [`gglib_core::telemetry::console_println`].
    ///
    /// Returns `Err` if Python is not found or setup fails.
    pub async fn prepare(notice: Option<&NoticeCallback>) -> Result<Self, EnvSetupError> {
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
        env.ensure_env_ready(notice).await?;

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
        venv_python_path(&self.env_dir)
    }

    /// Get the path to the helper script.
    pub fn script_path(&self) -> &Path {
        &self.script_path
    }

    // ------------------------------------------------------------------------
    // Internal methods
    // ------------------------------------------------------------------------

    async fn ensure_env_ready(&self, notice: Option<&NoticeCallback>) -> Result<(), EnvSetupError> {
        if !self.python_path().exists() {
            self.create_env(notice).await?;
        }

        if !self.marker_is_fresh()? {
            self.install_requirements(notice).await?;
            self.write_marker()?;
        }

        Ok(())
    }

    async fn create_env(&self, notice: Option<&NoticeCallback>) -> Result<(), EnvSetupError> {
        let bootstrap = find_bootstrap_python_validated().await?;

        // With a notice sink (the queued-download path), the note lands on
        // the bar itself and stays terse — no path, it wouldn't fit and
        // isn't actionable there. Without one (preflight, `model upgrade`),
        // fall back to a console line with the full path for context.
        notify(
            notice,
            "preparing fast downloader (first run, this can take a minute)…",
            &format!(
                "ℹ️  Creating Python environment for fast downloads at {}...",
                self.env_dir.display()
            ),
        );

        let mut cmd = async_cmd(&bootstrap);
        apply_python_subprocess_isolation(&mut cmd);
        // `.output()` rather than `.status()`: this pipes the child's stdout
        // and stderr instead of inheriting the parent's, so `python -m venv`
        // can never write raw bytes straight to the terminal. An inherited
        // handle would bypass indicatif entirely and corrupt any live
        // `MultiProgress` redraw the same way a stray `println!` does.
        // `run_python_command` below already pipes for the same reason.
        let output = cmd
            .arg("-m")
            .arg("venv")
            .arg(&self.env_dir)
            .output()
            .await
            .map_err(|e| EnvSetupError::CreateEnvFailed {
                path: self.env_dir.clone(),
                reason: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(EnvSetupError::CreateEnvFailed {
                path: self.env_dir.clone(),
                reason: venv_failure_reason(output.status, &output.stderr),
            });
        }

        Ok(())
    }

    async fn install_requirements(
        &self,
        notice: Option<&NoticeCallback>,
    ) -> Result<(), EnvSetupError> {
        notify(
            notice,
            "installing fast downloader dependencies…",
            "ℹ️  Installing fast download dependencies...",
        );

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

/// Path to the interpreter inside a venv rooted at `env_dir`.
fn venv_python_path(env_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        env_dir.join("Scripts").join("python.exe")
    } else {
        let bin = env_dir.join("bin");
        let python3 = bin.join("python3");
        if python3.exists() {
            python3
        } else {
            bin.join("python")
        }
    }
}

/// Whether the `hf_xet` accelerator's environment is already provisioned.
///
/// A file-existence check, deliberately: this decides whether a download uses
/// the accelerator, and it must not spawn a process, touch the network, or
/// create anything. A machine with no Python answers `false` and gets the
/// native download path — which is the whole point.
pub fn fast_helper_provisioned() -> bool {
    get_env_directory().is_ok_and(|dir| venv_python_path(&dir).exists())
}

/// Environment inputs to interpreter discovery, snapshotted so the candidate
/// list can be built and asserted on without touching the real environment.
///
/// `conda_prefix` is read from the *parent* process on purpose, and it is the
/// one thing here most likely to look like a bug. gglib does not run inside the
/// user's conda environment — [`apply_python_subprocess_isolation`] strips
/// `CONDA_PREFIX` from every child it spawns, and that must not change. This
/// reads the variable only to learn *where a Python lives on disk*; the
/// interpreter found there is then launched with the same scrubbed environment
/// as any other. Locating and inheriting are different things.
#[derive(Debug, Default, Clone)]
struct DiscoveryEnv {
    home: Option<PathBuf>,
    conda_prefix: Option<PathBuf>,
    pyenv_root: Option<PathBuf>,
}

impl DiscoveryEnv {
    fn from_process() -> Self {
        Self {
            home: dirs::home_dir(),
            conda_prefix: env::var_os("CONDA_PREFIX").map(PathBuf::from),
            pyenv_root: env::var_os("PYENV_ROOT").map(PathBuf::from),
        }
    }
}

/// The interpreter inside an installation prefix.
fn interpreter_in(prefix: &Path) -> PathBuf {
    if cfg!(windows) {
        prefix.join("python.exe")
    } else {
        prefix.join("bin").join("python3")
    }
}

/// Interpreter paths implied directly by `env`, in preference order.
///
/// Pure: every entry is a path that may or may not exist, and the caller
/// filters. An active conda environment comes first because a user who has one
/// activated has told us which Python they mean.
fn direct_candidates(env: &DiscoveryEnv) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(prefix) = &env.conda_prefix {
        out.push(interpreter_in(prefix));
    }

    if let Some(home) = &env.home {
        for prefix in CONDA_HOME_PREFIXES {
            out.push(interpreter_in(&home.join(prefix)));
        }
    }

    if cfg!(target_os = "macos") {
        // Homebrew's prefix differs by architecture and neither is guaranteed
        // to be on a non-interactive PATH.
        out.push(PathBuf::from("/opt/homebrew/bin/python3"));
        out.push(PathBuf::from("/usr/local/bin/python3"));
    }

    out
}

/// Directories whose immediate children are each an installed Python prefix,
/// in preference order.
///
/// pyenv's `versions/` and uv's managed-python store both have this shape.
/// Expanded by [`expand_version_stores`], which is where the I/O lives.
fn version_store_candidates(env: &DiscoveryEnv) -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(root) = &env.pyenv_root {
        out.push(root.join("versions"));
    }

    if let Some(home) = &env.home {
        out.push(home.join(".pyenv").join("versions"));
        out.push(home.join(".local").join("share").join("uv").join("python"));
    }

    out
}

/// Ordering key for a version-store entry: its leading numeric components.
///
/// Sorting these names as strings is wrong in the one case that matters most
/// right now — `3.9.18` sorts above `3.12.1` lexicographically, because `9`
/// beats `1` on the first differing character — and picking 3.9 over 3.12 is
/// the opposite of "newest". Entries that do not start with a number (pyenv
/// also stores `pypy3.10-7.3.15`, `miniconda3-4.7.12`) yield an empty key and
/// sort last, behind every plain version-numbered interpreter.
fn version_sort_key(name: &str) -> Vec<u64> {
    name.split(['.', '-'])
        .map(str::parse::<u64>)
        .take_while(Result::is_ok)
        .filter_map(Result::ok)
        .collect()
}

/// Expand each version store into the interpreters it holds, newest first.
///
/// Sorting is also what makes this deterministic. Directory iteration order is
/// filesystem-defined, so without it a machine with several pyenv versions
/// would build its environment against an arbitrary one and could pick a
/// different one after an unrelated reinstall.
fn expand_version_stores(stores: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();

    for store in stores {
        let Ok(entries) = fs::read_dir(store) else {
            continue;
        };

        let mut prefixes: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();

        prefixes.sort_by(|a, b| {
            let name = |p: &Path| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            };
            let (a, b) = (name(a), name(b));
            // Version descending, then name descending so equal keys (and the
            // unparseable tail) still have one fixed order.
            version_sort_key(&b)
                .cmp(&version_sort_key(&a))
                .then_with(|| b.cmp(&a))
        });

        out.extend(prefixes.iter().map(|prefix| interpreter_in(prefix)));
    }

    out
}

/// Every interpreter path worth validating, in preference order, deduplicated.
///
/// `PATH` comes before the manager-specific locations: if the user has arranged
/// for a `python3` to be the one they get in a shell, that is the answer, and
/// the rest of this list only exists for machines where they have not.
fn bootstrap_candidates(env: &DiscoveryEnv) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = PYTHON_CANDIDATES
        .iter()
        .filter_map(|name| which::which(name).ok())
        .collect();

    out.extend(direct_candidates(env));
    out.extend(expand_version_stores(&version_store_candidates(env)));

    let mut seen = std::collections::HashSet::new();
    out.retain(|path| seen.insert(path.clone()));
    out
}

/// Resolve the Windows `py -3` launcher to a real interpreter path.
///
/// The launcher is not itself an interpreter path, so it cannot go in the
/// candidate list; ask it where Python actually is.
#[cfg(target_os = "windows")]
async fn resolve_launcher_python() -> Option<PathBuf> {
    let mut cmd = async_cmd("py");
    apply_python_subprocess_isolation(&mut cmd);
    cmd.arg("-3")
        .arg("-c")
        .arg("import sys; print(sys.executable)");

    let output = cmd.output().await.ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(not(target_os = "windows"))]
#[allow(clippy::unused_async)] // Mirrors the Windows signature; there is no launcher here.
async fn resolve_launcher_python() -> Option<PathBuf> {
    None
}

/// Find a Python interpreter suitable for bootstrapping the environment.
///
/// The explicit override is absolute — a user who names an interpreter gets
/// that one or an error, never a silent substitution. Everything after it is a
/// search, and a candidate that fails validation is skipped rather than fatal:
/// a half-removed conda install should not stop a working pyenv one being
/// found.
async fn find_bootstrap_python_validated() -> Result<PathBuf, EnvSetupError> {
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

    let discovery = DiscoveryEnv::from_process();
    let mut candidates = bootstrap_candidates(&discovery);
    if let Some(launcher) = resolve_launcher_python().await {
        candidates.push(launcher);
    }

    let mut tried: Vec<String> = Vec::new();
    let mut last_error: Option<String> = None;

    for candidate in &candidates {
        if !candidate.exists() {
            continue;
        }

        tried.push(candidate.display().to_string());

        match validate_python_interpreter(candidate).await {
            Ok(_) => return Ok(candidate.clone()),
            Err(e) => last_error = Some(e.to_string()),
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

/// Surface a transient setup note: on the bar via `notice` when a sink is
/// wired up, otherwise as a console line with fuller context.
fn notify(notice: Option<&NoticeCallback>, bar_message: &str, console_message: &str) {
    if let Some(notice) = notice {
        notice(bar_message);
    } else {
        gglib_core::telemetry::console_println(console_message);
    }
}

/// Build the `CreateEnvFailed` reason for a failed `python -m venv`,
/// including captured stderr when there is any. Pulled out of `create_env`
/// so the formatting is testable without spawning a real process.
fn venv_failure_reason(status: std::process::ExitStatus, stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        format!("python -m venv exited with {status}")
    } else {
        format!("python -m venv exited with {status}: {stderr}")
    }
}

/// Run a Python command and check for success.
async fn run_python_command(python: &Path, args: &[&str]) -> Result<(), EnvSetupError> {
    let mut cmd = async_cmd(python);
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

/// Split the validation probe's stdout into `sys.executable` and the version.
///
/// Pulled out so the parsing is testable without an interpreter: the probe is
/// two `print` calls, and a candidate that answers with anything else (a shim
/// that logs a deprecation banner, say) must read as unusable rather than
/// panic or be silently accepted.
fn parse_validation_output(stdout: &str) -> Option<(String, (u32, u32))> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let executable = lines.next()?.to_string();
    let version = lines.next()?;

    let (major, minor) = version.split_once('.')?;
    Some((executable, (major.parse().ok()?, minor.parse().ok()?)))
}

/// Validate that the given Python interpreter can import the standard library
/// and is new enough for the requirements.
///
/// Returns the resolved `sys.executable` string on success.
async fn validate_python_interpreter(python: &Path) -> Result<String, EnvSetupError> {
    let mut cmd = async_cmd(python);
    apply_python_subprocess_isolation(&mut cmd);
    cmd.arg("-c").arg(
        "import encodings, sys; print(sys.executable); \
         print('%d.%d' % sys.version_info[:2])",
    );

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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some((executable, version)) = parse_validation_output(&stdout) else {
        return Err(EnvSetupError::PythonInvalid {
            path: python.to_path_buf(),
            reason: format!("unexpected probe output: {}", stdout.trim()),
        });
    };

    if version < MIN_PYTHON {
        return Err(EnvSetupError::PythonTooOld {
            path: python.to_path_buf(),
            found: format!("{}.{}", version.0, version.1),
            required: MIN_PYTHON,
        });
    }

    Ok(if executable.is_empty() {
        python.display().to_string()
    } else {
        executable
    })
}

/// Get the directory for the Python environment.
fn get_env_directory() -> Result<PathBuf, EnvSetupError> {
    let root = data_root().map_err(|e| EnvSetupError::DataRootFailed(e.to_string()))?;
    Ok(resolve_env_directory(&root))
}

/// Pick the environment directory under `root`, preferring the current path.
///
/// Split from [`get_env_directory`] so the precedence is testable against a
/// temp directory rather than the real data root. The legacy path wins only
/// when it is the only one present: once an environment exists at the current
/// path, that is the one every caller must agree on, including the
/// file-existence check in [`fast_helper_provisioned`].
fn resolve_env_directory(root: &Path) -> PathBuf {
    let current = root.join(ENV_PARENT_DIR).join(ENV_NAME);
    if current.exists() {
        return current;
    }

    let legacy = root.join(LEGACY_ENV_PARENT_DIR).join(ENV_NAME);
    if legacy.exists() {
        return legacy;
    }

    current
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

    /// A fresh machine has neither directory: the current path is what gets
    /// built, so it is what gets returned.
    #[test]
    fn resolve_env_directory_defaults_to_current_path() {
        let root = tempfile::tempdir().expect("tempdir");

        assert_eq!(
            resolve_env_directory(root.path()),
            root.path().join(ENV_PARENT_DIR).join(ENV_NAME)
        );
    }

    /// An install that predates the rename keeps its environment. Rebuilding
    /// it under the new name would re-download several hundred megabytes of
    /// wheels to end up in exactly the same place.
    #[test]
    fn resolve_env_directory_keeps_a_legacy_environment() {
        let root = tempfile::tempdir().expect("tempdir");
        let legacy = root.path().join(LEGACY_ENV_PARENT_DIR).join(ENV_NAME);
        fs::create_dir_all(&legacy).expect("create legacy env");

        assert_eq!(resolve_env_directory(root.path()), legacy);
    }

    /// Both present — which happens if a user provisions, downgrades, and
    /// provisions again — has to resolve the same way for every caller, or
    /// `fast_helper_provisioned` and the code that runs the interpreter would
    /// disagree about which environment is live. Current wins.
    #[test]
    fn resolve_env_directory_prefers_current_over_legacy() {
        let root = tempfile::tempdir().expect("tempdir");
        let current = root.path().join(ENV_PARENT_DIR).join(ENV_NAME);
        fs::create_dir_all(&current).expect("create current env");
        fs::create_dir_all(root.path().join(LEGACY_ENV_PARENT_DIR).join(ENV_NAME))
            .expect("create legacy env");

        assert_eq!(resolve_env_directory(root.path()), current);
    }

    /// An activated conda environment is the strongest signal available about
    /// which Python the user means, so it leads.
    #[test]
    fn direct_candidates_lead_with_the_active_conda_environment() {
        let env = DiscoveryEnv {
            home: Some(PathBuf::from("/home/u")),
            conda_prefix: Some(PathBuf::from("/opt/conda/envs/ml")),
            pyenv_root: None,
        };

        let candidates = direct_candidates(&env);

        assert_eq!(
            candidates[0],
            interpreter_in(Path::new("/opt/conda/envs/ml"))
        );
    }

    /// Every conda-family layout gets an entry; a user on miniforge should not
    /// have to care that the code was written against anaconda.
    #[test]
    fn direct_candidates_cover_every_conda_family_layout() {
        let env = DiscoveryEnv {
            home: Some(PathBuf::from("/home/u")),
            ..DiscoveryEnv::default()
        };

        let candidates = direct_candidates(&env);

        for prefix in CONDA_HOME_PREFIXES {
            let expected = interpreter_in(&PathBuf::from("/home/u").join(prefix));
            assert!(
                candidates.contains(&expected),
                "{prefix} missing from {candidates:?}"
            );
        }
    }

    /// Nothing set: no candidates and, more importantly, no panic and no
    /// path built from an empty prefix (`/bin/python3` would be a real file on
    /// some systems and the wrong answer on all of them).
    #[test]
    fn direct_candidates_are_empty_without_a_home_or_conda() {
        let candidates = direct_candidates(&DiscoveryEnv::default());

        #[cfg(target_os = "macos")]
        assert_eq!(candidates.len(), 2, "only the Homebrew prefixes");
        #[cfg(not(target_os = "macos"))]
        assert!(candidates.is_empty(), "got {candidates:?}");
    }

    #[test]
    fn version_stores_cover_pyenv_and_uv() {
        let env = DiscoveryEnv {
            home: Some(PathBuf::from("/home/u")),
            conda_prefix: None,
            pyenv_root: Some(PathBuf::from("/opt/pyenv")),
        };

        let stores = version_store_candidates(&env);

        assert!(stores.contains(&PathBuf::from("/opt/pyenv/versions")));
        assert!(stores.contains(&PathBuf::from("/home/u/.pyenv/versions")));
        assert!(stores.contains(&PathBuf::from("/home/u/.local/share/uv/python")));
    }

    /// Directory iteration order is filesystem-defined. Without the sort, a
    /// machine with several pyenv versions would build against an arbitrary
    /// one and could silently switch after an unrelated reinstall.
    ///
    /// `3.9.18` against `3.12.1` is the case that matters: sorted as strings
    /// it wins, and it is the older interpreter.
    #[test]
    fn expand_version_stores_orders_newest_first() {
        let store = tempfile::tempdir().expect("tempdir");
        for version in ["3.9.18", "3.12.1", "3.11.7"] {
            fs::create_dir_all(store.path().join(version).join("bin")).expect("create version");
        }

        let found = expand_version_stores(&[store.path().to_path_buf()]);

        assert_eq!(
            store_entry_names(&found),
            vec!["3.12.1", "3.11.7", "3.9.18"]
        );
    }

    /// pyenv keeps more than plain interpreters in `versions/`. Those entries
    /// are real candidates, but must never outrank a version-numbered one.
    #[test]
    fn expand_version_stores_sorts_non_cpython_entries_last() {
        let store = tempfile::tempdir().expect("tempdir");
        for entry in ["3.10.13", "pypy3.10-7.3.15", "miniconda3-4.7.12"] {
            fs::create_dir_all(store.path().join(entry).join("bin")).expect("create entry");
        }

        let found = expand_version_stores(&[store.path().to_path_buf()]);

        assert_eq!(store_entry_names(&found)[0], "3.10.13");
    }

    #[test]
    fn version_sort_key_stops_at_the_first_non_numeric_component() {
        assert_eq!(version_sort_key("3.12.1"), vec![3, 12, 1]);
        assert_eq!(version_sort_key("3.9.18"), vec![3, 9, 18]);
        assert!(version_sort_key("3.12.1") > version_sort_key("3.9.18"));
        assert_eq!(version_sort_key("pypy3.10-7.3.15"), Vec::<u64>::new());
        assert_eq!(version_sort_key("miniconda3-4.7.12"), Vec::<u64>::new());
    }

    /// The store entry name for each discovered interpreter (`<store>/<name>/bin/python3`).
    fn store_entry_names(interpreters: &[PathBuf]) -> Vec<String> {
        interpreters
            .iter()
            .filter_map(|p| {
                let prefix = if cfg!(windows) {
                    p.parent()?
                } else {
                    p.parent()?.parent()?
                };
                prefix.file_name()
            })
            .map(|n| n.to_string_lossy().into_owned())
            .collect()
    }

    /// A store that does not exist is the common case on most machines.
    #[test]
    fn expand_version_stores_ignores_missing_directories() {
        assert!(expand_version_stores(&[PathBuf::from("/nonexistent/pyenv/versions")]).is_empty());
    }

    /// The same interpreter can be reachable as `python3`, `python3.12`, and a
    /// pyenv path at once. Validating it three times is wasted subprocesses.
    #[test]
    fn bootstrap_candidates_are_deduplicated() {
        let candidates = bootstrap_candidates(&DiscoveryEnv::from_process());

        let mut seen = std::collections::HashSet::new();
        for candidate in &candidates {
            assert!(seen.insert(candidate), "duplicate candidate {candidate:?}");
        }
    }

    #[test]
    fn parse_validation_output_reads_executable_and_version() {
        let parsed = parse_validation_output("/usr/bin/python3\n3.12\n");

        assert_eq!(parsed, Some(("/usr/bin/python3".to_string(), (3, 12))));
    }

    /// A shim that prints a banner before the answer is not something to
    /// guess about.
    #[test]
    fn parse_validation_output_rejects_unparseable_output() {
        assert_eq!(parse_validation_output(""), None);
        assert_eq!(parse_validation_output("/usr/bin/python3\n"), None);
        assert_eq!(
            parse_validation_output("/usr/bin/python3\nthree.twelve\n"),
            None
        );
    }

    /// The floor exists so the failure lands at discovery, with a candidate
    /// still to try, rather than during `pip install` with nothing left.
    #[test]
    fn version_gate_rejects_below_the_floor_and_accepts_above() {
        assert!((3, 8) < MIN_PYTHON);
        assert!((3, 9) >= MIN_PYTHON);
        assert!((3, 12) >= MIN_PYTHON);
        assert!((4, 0) >= MIN_PYTHON);
    }

    #[test]
    fn python_too_old_error_names_both_versions() {
        let err = EnvSetupError::PythonTooOld {
            path: PathBuf::from("/usr/bin/python3"),
            found: "3.8".to_string(),
            required: (3, 9),
        };

        let msg = err.to_string();
        assert!(msg.contains("/usr/bin/python3"), "{msg}");
        assert!(msg.contains("3.8"), "{msg}");
        assert!(msg.contains("3.9"), "{msg}");
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

    /// `create_env` now runs `python -m venv` via `.output()` instead of
    /// `.status()` specifically so its stderr can be captured into the
    /// error instead of being inherited straight to the terminal (where it
    /// could corrupt a live `MultiProgress` redraw). This is the formatting
    /// half of that change, tested without spawning a process: a fake
    /// `ExitStatus` plus captured stderr bytes.
    #[cfg(unix)]
    #[test]
    fn venv_failure_reason_includes_captured_stderr() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(1 << 8); // exit code 1
        let reason = venv_failure_reason(status, b"NotADirectoryError: [Errno 20]\n");

        assert!(reason.contains("exited with"));
        assert!(reason.contains("NotADirectoryError"));
    }

    /// Empty stderr (e.g. the process was killed by a signal before writing
    /// anything) must not produce a dangling ": " with nothing after it.
    #[cfg(unix)]
    #[test]
    fn venv_failure_reason_omits_colon_when_stderr_is_empty() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(1 << 8);
        let reason = venv_failure_reason(status, b"");

        // No trailing ": " separator (which would precede an empty stderr
        // section) — just the bare "exited with <status>" whatever ExitStatus's
        // own Display happens to contain.
        assert_eq!(reason, format!("python -m venv exited with {status}"));
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
        let mut cmd = async_cmd(python);

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
