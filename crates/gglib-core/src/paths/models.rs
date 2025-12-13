//! Models directory resolution.
//!
//! Provides utilities for resolving the models directory from explicit paths,
//! environment variables, or platform defaults.

use std::env;
use std::path::PathBuf;

use super::error::PathError;
use super::platform::normalize_user_path;

/// Default relative location for downloaded models under the user's home directory.
pub const DEFAULT_MODELS_DIR_RELATIVE: &str = ".local/share/llama_models";

/// How the models directory was derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelsDirSource {
    /// The user passed an explicit path (e.g., CLI flag or GUI form).
    Explicit,
    /// The path came from environment variables / `.env`.
    EnvVar,
    /// Fallback default (`~/.local/share/llama_models`).
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
/// Defaults to `~/.local/share/llama_models`.
pub fn default_models_dir() -> Result<PathBuf, PathError> {
    let home = dirs::home_dir().ok_or(PathError::NoHomeDir)?;
    Ok(home.join(DEFAULT_MODELS_DIR_RELATIVE))
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

    if let Ok(env_path) = env::var("GGLIB_MODELS_DIR") {
        if !env_path.trim().is_empty() {
            return Ok(ModelsDirResolution {
                path: normalize_user_path(&env_path)?,
                source: ModelsDirSource::EnvVar,
            });
        }
    }

    Ok(ModelsDirResolution {
        path: default_models_dir()?,
        source: ModelsDirSource::Default,
    })
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;

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
