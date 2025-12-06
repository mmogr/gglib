//! Path utilities for gglib CLI.
//!
//! These utilities handle data directories and user-configurable locations.
//! They are pure functions of config/env and do not depend on CLI-specific flags.

use anyhow::{Context, Result, anyhow, bail};
use dirs::home_dir;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::env;

/// Default relative location for downloaded models under the user's home directory.
pub const DEFAULT_MODELS_DIR_RELATIVE: &str = ".local/share/llama_models";

/// Get the root directory for application data (database, config).
///
/// This is unified across Development and Release builds to ensure that
/// users see the same data regardless of how they run the application.
/// Defaults to system data directory (e.g., ~/.local/share/gglib).
pub fn get_data_root() -> Result<PathBuf> {
    // 1. Runtime override (highest priority)
    if let Ok(path) = env::var("GGLIB_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    // 2. Try local repo (e.g. make setup)
    if let Some(repo) = detect_local_repo() {
        return Ok(repo);
    }

    // 3. Default to system data directory
    let data_dir =
        dirs::data_local_dir().ok_or_else(|| anyhow!("Cannot determine system data directory"))?;

    #[cfg(target_os = "windows")]
    let root = data_dir.join("gglib");

    #[cfg(not(target_os = "windows"))]
    let root = data_dir.join("gglib");

    // Ensure it exists
    if !root.exists() {
        fs::create_dir_all(&root).context("Failed to create app data directory")?;
    }

    Ok(root)
}

/// Get the root directory for application resources (binaries, static assets).
///
/// In Development (debug builds), this returns the repository root to use local artifacts.
/// In Release builds, this defaults to the data root, assuming resources are installed there.
pub fn get_resource_root() -> Result<PathBuf> {
    // 1. Runtime override
    if let Ok(path) = env::var("GGLIB_RESOURCE_DIR") {
        return Ok(PathBuf::from(path));
    }

    // 2. Try local repo
    if let Some(repo) = detect_local_repo() {
        return Ok(repo);
    }

    // 3. Fallback to system data directory (Pre-built binary / Installed)
    get_data_root()
}

/// Get the gglib data directory.
///
/// Returns the `.llama/` directory containing helper binaries.
/// In dev, this is in the repo. In release, this is in the user data dir.
pub fn get_gglib_data_dir() -> Result<PathBuf> {
    Ok(get_resource_root()?.join(".llama"))
}

/// Get the path to the gglib database file.
///
/// Returns the path to `gglib.db` in the user data directory.
/// This is shared between dev and release builds.
pub fn get_database_path() -> Result<PathBuf> {
    let data_dir = get_data_root()?.join("data");
    std::fs::create_dir_all(&data_dir).context("Failed to create data directory")?;

    Ok(data_dir.join("gglib.db"))
}

/// Get the path to the managed llama-server binary.
pub fn get_llama_server_path() -> Result<PathBuf> {
    let gglib_dir = get_gglib_data_dir()?;

    #[cfg(target_os = "windows")]
    let binary_name = "llama-server.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "llama-server";

    Ok(gglib_dir.join("bin").join(binary_name))
}

/// Get the path to the managed llama-cli binary.
pub fn get_llama_cli_path() -> Result<PathBuf> {
    let gglib_dir = get_gglib_data_dir()?;

    #[cfg(target_os = "windows")]
    let binary_name = "llama-cli.exe";

    #[cfg(not(target_os = "windows"))]
    let binary_name = "llama-cli";

    Ok(gglib_dir.join("bin").join(binary_name))
}

/// Get the path to the llama.cpp repository directory.
pub fn get_llama_cpp_dir() -> Result<PathBuf> {
    let gglib_dir = get_gglib_data_dir()?;
    Ok(gglib_dir.join("llama.cpp"))
}

/// Get the path to the llama build configuration file.
pub fn get_llama_config_path() -> Result<PathBuf> {
    let gglib_dir = get_gglib_data_dir()?;
    Ok(gglib_dir.join("llama-config.json"))
}

/// Location of the `.env` file that stores user overrides.
pub fn env_file_path() -> Result<PathBuf> {
    Ok(get_data_root()?.join(".env"))
}

/// Return the platform-specific default models directory (`~/.local/share/llama_models`).
pub fn default_models_dir() -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    Ok(home.join(DEFAULT_MODELS_DIR_RELATIVE))
}

/// Resolve the models directory and record how it was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsDirSource {
    /// The user passed an explicit path (e.g., CLI flag or GUI form).
    Explicit,
    /// The path came from environment variables / `.env`.
    EnvVar,
    /// Fallback default (`~/.local/share/llama_models`).
    Default,
}

/// Resolution result for the models directory helper.
#[derive(Debug, Clone)]
pub struct ModelsDirResolution {
    pub path: PathBuf,
    pub source: ModelsDirSource,
}

/// Resolve the models directory from an explicit override, env var, or default.
pub fn resolve_models_dir(explicit: Option<&str>) -> Result<ModelsDirResolution> {
    if let Some(path_str) = explicit {
        return Ok(ModelsDirResolution {
            path: normalize_user_path(path_str)?,
            source: ModelsDirSource::Explicit,
        });
    }

    match env::var("GGLIB_MODELS_DIR") {
        Ok(env_path) if !env_path.trim().is_empty() => {
            return Ok(ModelsDirResolution {
                path: normalize_user_path(&env_path)?,
                source: ModelsDirSource::EnvVar,
            });
        }
        _ => {}
    };

    Ok(ModelsDirResolution {
        path: default_models_dir()?,
        source: ModelsDirSource::Default,
    })
}

/// Strategy for how to handle missing directories when ensuring they exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryCreationStrategy {
    /// Create directories automatically if they are missing.
    AutoCreate,
    /// Do not create directories; return an error if missing.
    Disallow,
}

/// Ensure the provided directory exists and is writable according to the chosen strategy.
pub fn ensure_directory(path: &Path, strategy: DirectoryCreationStrategy) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!("{} exists but is not a directory", path.display());
        }
    } else {
        match strategy {
            DirectoryCreationStrategy::AutoCreate => {
                fs::create_dir_all(path)
                    .with_context(|| format!("Failed to create directory {}", path.display()))?;
            }
            DirectoryCreationStrategy::Disallow => {
                bail!("Directory {} does not exist", path.display());
            }
        }
    }

    verify_writable(path)?;
    Ok(())
}

/// Persist an environment value into `.env`.
pub fn persist_env_value(key: &str, value: &str) -> Result<()> {
    let env_path = env_file_path()?;
    let mut lines: Vec<String> = if env_path.exists() {
        fs::read_to_string(&env_path)?
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let mut updated = false;

    let mut output: Vec<String> = Vec::with_capacity(lines.len() + 1);
    for line in lines.drain(..) {
        match line.split_once('=') {
            Some((lhs, _)) if lhs.trim() == key => {
                if !updated {
                    output.push(format!("{key}={value}"));
                    updated = true;
                }
            }
            _ => output.push(line),
        }
    }

    if !updated {
        if !output.is_empty() && !output.last().unwrap().is_empty() {
            output.push(String::new());
        }
        output.push(format!("{key}={value}"));
    }

    if !output.is_empty() && !output.last().unwrap().is_empty() {
        output.push(String::new());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&env_path)
        .with_context(|| format!("Failed to open {}", env_path.display()))?;
    let content = output.join("\n");
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Persist the selected models directory into `.env`.
pub fn persist_models_dir(path: &Path) -> Result<()> {
    let serialized = path.to_string_lossy().to_string();
    persist_env_value("GGLIB_MODELS_DIR", &serialized)
}

/// Verify a directory is writable by attempting to create a test file.
pub fn verify_writable(path: &Path) -> Result<()> {
    let test_file = path.join(".gglib_write_test");
    let result = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&test_file);

    match result {
        Ok(mut file) => {
            file.write_all(b"test")?;
            drop(file);
            let _ = fs::remove_file(&test_file);
            Ok(())
        }
        Err(err) => bail!("Directory {} is not writable ({})", path.display(), err),
    }
}

/// Normalize a user-provided path, expanding `~` and making it absolute.
fn normalize_user_path(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Path cannot be empty");
    }

    let expanded = if trimmed.starts_with("~/") || trimmed == "~" {
        let home = home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
        if trimmed == "~" {
            home
        } else {
            home.join(trimmed.trim_start_matches("~/"))
        }
    } else {
        PathBuf::from(trimmed)
    };

    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        Ok(env::current_dir()?.join(expanded))
    }
}

/// Helper to detect if we are running from the local repository.
///
/// Returns `Some(path)` if we are in a dev environment or running a release build
/// from within the source repo (e.g. `make setup`).
/// Returns `None` if we are running a standalone binary (e.g. installed via cargo install,
/// downloaded, or bundled as a macOS/Linux app).
fn detect_local_repo() -> Option<PathBuf> {
    let repo_root = PathBuf::from(env!("GGLIB_REPO_ROOT"));

    #[cfg(debug_assertions)]
    {
        // In debug mode, always assume we want to use the repo we are building from
        Some(repo_root)
    }

    #[cfg(not(debug_assertions))]
    {
        // In release mode, check if this binary was built from a local repo.
        // We use multiple strategies to detect this:

        // First, verify the repo path exists and looks like a valid gglib repo
        if !repo_root.exists()
            || (!repo_root.join(".git").exists() && !repo_root.join("Cargo.toml").exists())
        {
            return None;
        }

        // Strategy 1: Check for the .gglib_repo_path marker file created by build.rs
        // This file is written at compile time and contains the repo path.
        // If it exists and matches GGLIB_REPO_ROOT, this binary was built from this repo.
        let marker_file = repo_root.join("data").join(".gglib_repo_path");
        if marker_file.exists() {
            if let Ok(contents) = fs::read_to_string(&marker_file) {
                let marker_path = contents.trim();
                // Verify the marker matches the compile-time repo root
                if marker_path == repo_root.to_string_lossy() {
                    return Some(repo_root);
                }
            }
        }

        // Strategy 2 (fallback): Check if executable is inside the repo
        // This handles cases where the marker file might be missing but
        // we're running directly from target/release/
        if let Ok(exe_path) = env::current_exe() {
            if let Ok(canonical_exe) = exe_path.canonicalize() {
                if let Ok(canonical_repo) = repo_root.canonicalize() {
                    if canonical_exe.starts_with(&canonical_repo) {
                        return Some(repo_root);
                    }
                }
            }
        }

        None
    }
}

/// Check if we are running from a pre-built binary (not from the source repo).
///
/// Returns `true` if this is a standalone/installed binary (e.g., downloaded release,
/// `cargo install`). Returns `false` if running from the source repository (e.g.,
/// `cargo run`, `make setup`).
pub fn is_prebuilt_binary() -> bool {
    detect_local_repo().is_none()
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_gglib_data_dir() {
        let result = get_gglib_data_dir();
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with(".llama"));
    }

    #[test]
    fn test_get_llama_server_path() {
        let result = get_llama_server_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        #[cfg(target_os = "windows")]
        assert!(path.to_string_lossy().ends_with("llama-server.exe"));

        #[cfg(not(target_os = "windows"))]
        assert!(path.to_string_lossy().ends_with("llama-server"));
    }

    #[test]
    fn test_default_models_dir_contains_relative() {
        let dir = default_models_dir().unwrap();
        assert!(dir.to_string_lossy().contains(DEFAULT_MODELS_DIR_RELATIVE));
    }

    #[test]
    fn test_resolve_models_dir_prefers_explicit() {
        let prev = env::var("GGLIB_MODELS_DIR").ok();
        unsafe {
            env::set_var("GGLIB_MODELS_DIR", "/tmp/env-value");
        }
        let resolved = resolve_models_dir(Some("/tmp/explicit")).unwrap();
        assert_eq!(resolved.source, ModelsDirSource::Explicit);
        assert!(resolved.path.ends_with("explicit"));
        restore_env("GGLIB_MODELS_DIR", prev);
    }

    #[test]
    fn test_resolve_models_dir_env_value() {
        let prev = env::var("GGLIB_MODELS_DIR").ok();
        unsafe {
            env::set_var("GGLIB_MODELS_DIR", "/tmp/from-env");
        }
        let resolved = resolve_models_dir(None).unwrap();
        assert_eq!(resolved.source, ModelsDirSource::EnvVar);
        assert!(resolved.path.ends_with("from-env"));
        restore_env("GGLIB_MODELS_DIR", prev);
    }

    #[test]
    fn test_persist_models_dir_writes_env_file() {
        let temp = tempdir().unwrap();
        let prev = env::var("GGLIB_DATA_DIR").ok();
        unsafe {
            env::set_var("GGLIB_DATA_DIR", temp.path());
        }

        let models_dir = temp.path().join("models");
        persist_models_dir(&models_dir).unwrap();

        let env_contents = fs::read_to_string(temp.path().join(".env")).unwrap();
        assert!(env_contents.contains("GGLIB_MODELS_DIR"));
        assert!(env_contents.contains(models_dir.to_string_lossy().as_ref()));

        restore_env("GGLIB_DATA_DIR", prev);
    }

    fn restore_env(key: &str, previous: Option<String>) {
        if let Some(value) = previous {
            unsafe {
                env::set_var(key, value);
            }
        } else {
            unsafe {
                env::remove_var(key);
            }
        }
    }
}
