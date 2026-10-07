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
fn env_file_path() -> Result<PathBuf, PathError> {
    Ok(data_root()?.join(".env"))
}

/// `value` as the right-hand side of a `.env` line that reads back to the
/// same string whatever it holds: in double quotes, with a backslash before
/// each backslash, double quote and dollar sign, and a line break as `\n`.
///
/// Written bare, a value with a space in it is a line the loader refuses,
/// and it loads nothing from there on: every line after it is lost as well.
fn quoted(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '\\' | '"' | '$' => quoted.extend(['\\', c]),
            '\n' => quoted.push_str("\\n"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// Whether `line` assigns `key`, with or without the `export` the parser
/// allows before a key.
fn assigns(line: &str, key: &str) -> bool {
    line.split_once('=').is_some_and(|(name, _)| {
        let name = name.trim();
        let exported = name
            .strip_prefix("export")
            .filter(|rest| rest.starts_with(char::is_whitespace));
        exported.map_or(name, str::trim_start) == key
    })
}

/// Persist a key=value pair into the `.env` file, the value [`quoted`].
///
/// If the key already exists, its value is updated.
/// If the key doesn't exist, it is appended to the file.
fn persist_env_value(key: &str, value: &str) -> Result<(), PathError> {
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

    let assignment = format!("{key}={}", quoted(value));
    let mut updated = false;
    let mut output: Vec<String> = Vec::with_capacity(lines.len() + 1);

    for line in lines {
        if !assigns(&line, key) {
            output.push(line);
        } else if !updated {
            output.push(assignment.clone());
            updated = true;
        }
    }

    if !updated {
        if !output.is_empty() && !output.last().unwrap().is_empty() {
            output.push(String::new());
        }
        output.push(assignment);
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

/// The value the `.env` file holds for `key`: the first one the loader's own
/// parser reads for it, so a value means the same here as when the file is
/// loaded into a process's environment.
///
/// A line that parser refuses is skipped here, where loading stops at it.
/// `None` when the file is absent or unreadable or assigns no such key: a
/// missing override is not an error.
fn read_env_value(key: &str) -> Option<String> {
    let contents = fs::read_to_string(env_file_path().ok()?).ok()?;
    dotenvy::from_read_iter(contents.as_bytes())
        .filter_map(Result::ok)
        .find_map(|(name, value)| (name == key).then_some(value))
}

/// The key the models directory is stored under, in the environment and in
/// the `.env` file alike.
pub(super) const MODELS_DIR_KEY: &str = "GGLIB_MODELS_DIR";

/// Persist the selected models directory into `.env`.
pub(super) fn persist_models_dir(path: &Path) -> Result<(), PathError> {
    let serialized = path.to_string_lossy().to_string();
    persist_env_value(MODELS_DIR_KEY, &serialized)
}

/// The models directory [`persist_models_dir`] stored, if `.env` holds one.
/// A blank one is none, as it is in the environment.
pub(super) fn persisted_models_dir() -> Option<String> {
    read_env_value(MODELS_DIR_KEY).filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
