//! Models directory resolution, and the canonical form of a model file path.
//!
//! Provides utilities for resolving the models directory from explicit paths,
//! environment variables, or platform defaults, plus the single definition of
//! what makes two paths "the same model file" — see
//! [`canonical_model_path`].

use std::env;
use std::path::{Path, PathBuf};

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
/// 3. Default models directory (`~/.local/share/llama_models`)
pub fn resolve_models_dir(explicit: Option<&str>) -> Result<ModelsDirResolution, PathError> {
    if let Some(path_str) = explicit {
        return Ok(ModelsDirResolution {
            path: normalize_user_path(path_str)?,
            source: ModelsDirSource::Explicit,
        });
    }

    if let Ok(env_path) = env::var("GGLIB_MODELS_DIR")
        && !env_path.trim().is_empty()
    {
        return Ok(ModelsDirResolution {
            path: normalize_user_path(&env_path)?,
            source: ModelsDirSource::EnvVar,
        });
    }

    Ok(ModelsDirResolution {
        path: default_models_dir()?,
        source: ModelsDirSource::Default,
    })
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
mod tests {
    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};

    #[test]
    fn test_default_models_dir_platform_path() {
        let dir = default_models_dir().unwrap();
        let path_str = dir.to_string_lossy();
        // On Windows the path should be under %LOCALAPPDATA% and use native
        // separators throughout — no forward-slash fragments.
        #[cfg(target_os = "windows")]
        {
            assert!(
                path_str.contains("llama_models"),
                "Expected 'llama_models' in path: {path_str}"
            );
            assert!(
                !path_str.contains('/'),
                "Path must not contain forward slashes on Windows: {path_str}"
            );
        }
        // On non-Windows the path should sit under ~/.local/share/llama_models.
        #[cfg(not(target_os = "windows"))]
        assert!(
            path_str.contains(DEFAULT_MODELS_DIR_RELATIVE),
            "Expected '{DEFAULT_MODELS_DIR_RELATIVE}' in path: {path_str}"
        );
    }

    #[test]
    fn test_resolve_models_dir_prefers_explicit() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/env-value");
        let resolved = resolve_models_dir(Some("/tmp/explicit")).unwrap();
        assert_eq!(resolved.source, ModelsDirSource::Explicit);
        assert!(resolved.path.ends_with("explicit"));
    }

    #[test]
    fn test_resolve_models_dir_env_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _env = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/from-env");
        let resolved = resolve_models_dir(None).unwrap();
        assert_eq!(resolved.source, ModelsDirSource::EnvVar);
        assert!(resolved.path.ends_with("from-env"));
    }

    /// Two spellings of one file resolve to one answer. This is the property
    /// the model key, the stored column and the duplicate lookup all lean on;
    /// if it stops holding, a re-add silently merges two models into one row.
    #[test]
    fn canonical_model_path_agrees_across_spellings_of_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Model.gguf");
        std::fs::File::create(&file).unwrap();

        let direct = canonical_model_path(&file).unwrap();
        let indirect = canonical_model_path(&dir.path().join(".").join("Model.gguf")).unwrap();

        assert_eq!(direct, indirect);
    }

    /// The fallible form reports a path it cannot resolve instead of handing
    /// back the literal one. A caller that treats "cannot resolve" as "not a
    /// duplicate" reinstates the silent overwrite, so the error has to be
    /// reachable.
    #[test]
    fn canonical_model_path_reports_a_path_that_does_not_resolve() {
        let dir = tempfile::tempdir().unwrap();
        assert!(canonical_model_path(&dir.path().join("Absent.gguf")).is_err());
    }

    /// The string form is the one the database column stores, and it keeps
    /// the literal path when the file is gone so an existing row still
    /// round-trips.
    #[test]
    fn canonical_model_path_string_falls_back_to_the_literal_path() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("Absent.gguf");

        assert_eq!(
            canonical_model_path_string(&absent),
            absent.to_string_lossy()
        );
    }
}
