//! Configuration file utilities.
//!
//! Provides functions for reading and writing the `.env` file
//! that stores user configuration overrides.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::error::PathError;
use super::platform::data_root;

/// Location of the `.env` file that stores user overrides.
pub fn env_file_path() -> Result<PathBuf, PathError> {
    Ok(data_root()?.join(".env"))
}

/// Persist a key=value pair into the `.env` file.
///
/// If the key already exists, its value is updated.
/// If the key doesn't exist, it is appended to the file.
pub fn persist_env_value(key: &str, value: &str) -> Result<(), PathError> {
    let env_path = env_file_path()?;

    let lines: Vec<String> = if env_path.exists() {
        fs::read_to_string(&env_path)
            .map_err(|e| PathError::EnvFileError {
                path: env_path.clone(),
                reason: e.to_string(),
            })?
            .lines()
            .map(std::string::ToString::to_string)
            .collect()
    } else {
        Vec::new()
    };

    let mut updated = false;
    let mut output: Vec<String> = Vec::with_capacity(lines.len() + 1);

    for line in lines {
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

    // Ensure file ends with newline
    if !output.is_empty() && !output.last().unwrap().is_empty() {
        output.push(String::new());
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&env_path)
        .map_err(|e| PathError::EnvFileError {
            path: env_path.clone(),
            reason: e.to_string(),
        })?;

    let content = output.join("\n");
    file.write_all(content.as_bytes())
        .map_err(|e| PathError::EnvFileError {
            path: env_path,
            reason: e.to_string(),
        })?;

    Ok(())
}

/// Persist the selected models directory into `.env`.
pub fn persist_models_dir(path: &Path) -> Result<(), PathError> {
    let serialized = path.to_string_lossy().to_string();
    persist_env_value("GGLIB_MODELS_DIR", &serialized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};
    use tempfile::tempdir;

    #[test]
    fn test_persist_models_dir_writes_env_file() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();

        // Capture and restore GGLIB_DATA_DIR in a guard to ensure cleanup
        let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());

        let models_dir = temp.path().join("models");
        persist_models_dir(&models_dir).unwrap();

        let env_contents = fs::read_to_string(temp.path().join(".env")).unwrap();
        assert!(env_contents.contains("GGLIB_MODELS_DIR"));
        assert!(env_contents.contains(models_dir.to_string_lossy().as_ref()));

        // _env_guard will restore the original value when dropped
    }
}
