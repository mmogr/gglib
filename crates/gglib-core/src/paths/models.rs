//! Models directory resolution, and the canonical form of a model file path.
//!
//! Provides utilities for resolving the models directory from explicit paths,
//! environment variables, or platform defaults, plus the single definition of
//! what makes two paths "the same model file" — see
//! [`canonical_model_path`].

use std::env;
use std::path::{Path, PathBuf};

use super::config::{MODELS_DIR_KEY, persist_models_dir, persisted_models_dir};
use super::ensure::{DirectoryCreationStrategy, ensure_directory};
use super::error::PathError;
use super::platform::normalize_user_path;

/// Default relative location for downloaded models on non-Windows platforms.
#[cfg(not(target_os = "windows"))]
pub const DEFAULT_MODELS_DIR_RELATIVE: &str = ".local/share/llama_models";

/// How the models directory was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsDirSource {
    /// The user passed an explicit path (e.g., CLI flag or GUI form).
    Explicit,
    /// The path came from environment variables / `.env`.
    EnvVar,
    /// Fallback default (`~/.local/share/llama_models` on Linux/macOS,
    /// `%LOCALAPPDATA%\llama_models` on Windows).
    Default,
}

/// Resolution result for the models directory.
#[derive(Debug, Clone)]
pub struct ModelsDirResolution {
    /// The resolved path to the models directory.
    pub path: PathBuf,
    /// How the path was determined.
    pub source: ModelsDirSource,
}

/// Return the platform-specific default models directory.
///
/// - **Windows**: `%LOCALAPPDATA%\llama_models` (e.g. `C:\Users\name\AppData\Local\llama_models`)
/// - **macOS / Linux**: `~/.local/share/llama_models`
pub fn default_models_dir() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    {
        let local_app_data = dirs::data_local_dir().ok_or(PathError::NoDataDir)?;
        Ok(local_app_data.join("llama_models"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = dirs::home_dir().ok_or(PathError::NoHomeDir)?;
        Ok(home.join(DEFAULT_MODELS_DIR_RELATIVE))
    }
}

/// Resolve the models directory from an explicit override, env var, or default.
///
/// Resolution order:
/// 1. Explicit path provided by caller (highest priority)
/// 2. `GGLIB_MODELS_DIR` environment variable
/// 3. The directory [`set_models_dir`] stored in the data root's `.env`
/// 4. Default models directory (`~/.local/share/llama_models`)
///
/// Step 3 reads the file itself. A process loads a `.env` into its
/// environment only from the directory it was started in or one above it,
/// which holds the data root for a run from the checkout and seldom for an
/// installed one, so a stored directory would otherwise be lost wherever the
/// two differ.
pub fn resolve_models_dir(explicit: Option<&str>) -> Result<ModelsDirResolution, PathError> {
    if let Some(path_str) = explicit {
        return Ok(ModelsDirResolution {
            path: normalize_user_path(path_str)?,
            source: ModelsDirSource::Explicit,
        });
    }

    let from_env = env::var(MODELS_DIR_KEY)
        .ok()
        .filter(|path| !path.trim().is_empty());
    if let Some(stored) = from_env.or_else(persisted_models_dir) {
        return Ok(ModelsDirResolution {
            path: normalize_user_path(&stored)?,
            source: ModelsDirSource::EnvVar,
        });
    }

    Ok(ModelsDirResolution {
        path: default_models_dir()?,
        source: ModelsDirSource::Default,
    })
}

/// Make `path` the models directory: resolve it, create it as `strategy`
/// allows, and store it for every later run.
///
/// The one way a surface changes the directory. Storing a path that was
/// never resolved or created, or creating one that is never stored, is how
/// two surfaces come to disagree about where models live.
pub fn set_models_dir(
    path: &str,
    strategy: DirectoryCreationStrategy,
) -> Result<ModelsDirResolution, PathError> {
    let resolved = resolve_models_dir(Some(path))?;
    ensure_directory(&resolved.path, strategy)?;
    persist_models_dir(&resolved.path)?;
    Ok(resolved)
}

/// Resolve `path` to the one form the library identifies a model file by.
///
/// Three separate places have to agree about what "the same file" means: the
/// `file_path` column a model is stored under, the `model_key` that decides
/// whether an insert is really an update, and the duplicate lookup an
/// explicit add performs before inserting. While they disagreed, the failure
/// was silent and destructive — two *different* files sharing a relative name
/// (`model.gguf` in two directories) hashed to one key, the duplicate check
/// compared resolved paths and saw no match, and the UPSERT merged them into
/// a single row carrying the first file's name and the second file's path.
///
/// Everything that needs that answer resolves it here, so the three cannot
/// drift apart again.
///
/// # Errors
///
/// Returns the underlying [`std::io::Error`] when the path cannot be
/// resolved — most often because no file exists there.
///
/// Fallible on purpose. The infallible "canonicalise, or keep the literal
/// path" shape reads as the convenient one and is precisely the shape of the
/// bug: a caller asking *is this file already in the library?* gets a literal
/// path back, compares it against a stored canonical one, matches nothing,
/// and reports "no duplicate" for a file that is plainly there. Callers for
/// which this genuinely cannot fail have already established that the file
/// exists; they should say so by propagating rather than by swallowing.
pub fn canonical_model_path(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

/// The canonical path as the string the `file_path` column stores.
///
/// Falls back to the literal path when the file cannot be resolved, because a
/// row whose file has since been deleted still has to round-trip through the
/// database. Reach for [`canonical_model_path`] anywhere a failure to resolve
/// should be visible to the caller rather than papered over.
#[must_use]
pub fn canonical_model_path_string(path: &Path) -> String {
    canonical_model_path(path).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |resolved| resolved.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
