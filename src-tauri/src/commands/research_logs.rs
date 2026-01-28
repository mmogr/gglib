//! Research log file commands.
//!
//! Commands for persisting deep research logs to disk in NDJSON format.
//! This ensures logs survive app crashes/refreshes during debugging.

use gglib_core::paths::data_root;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Log directory name under data root.
const LOG_DIR: &str = "research_logs";

/// Get the research logs directory path.
fn research_logs_dir() -> Result<PathBuf, String> {
    let root = data_root().map_err(|e| format!("Failed to get data root: {}", e))?;
    Ok(root.join(LOG_DIR))
}

/// Initialize the research logs directory.
///
/// Creates the directory if it doesn't exist.
#[tauri::command]
pub fn init_research_logs() -> Result<(), String> {
    let log_dir = research_logs_dir()?;

    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    Ok(())
}

/// Append a log line to a session's log file.
///
/// Creates the file if it doesn't exist. Appends if it does.
/// The line should already include a newline (NDJSON format).
#[tauri::command]
pub fn append_research_log(session_id: String, line: String) -> Result<(), String> {
    // Sanitize session_id to prevent path traversal
    let safe_session_id = session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(100) // Limit length
        .collect::<String>();

    if safe_session_id.is_empty() {
        return Err("Invalid session ID".to_string());
    }

    let log_dir = research_logs_dir()?;

    // Ensure directory exists
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    let file_path = log_dir.join(format!("{}.ndjson", safe_session_id));

    // Open file in append mode (creates if doesn't exist)
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;

    // Write the line
    file.write_all(line.as_bytes())
        .map_err(|e| format!("Failed to write to log file: {}", e))?;

    Ok(())
}

/// Get the path to a session's log file.
///
/// Used by the frontend to display log file location.
#[tauri::command]
pub fn get_research_log_path(session_id: String) -> Result<String, String> {
    let safe_session_id = session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .take(100)
        .collect::<String>();

    if safe_session_id.is_empty() {
        return Err("Invalid session ID".to_string());
    }

    let log_dir = research_logs_dir()?;
    let file_path = log_dir.join(format!("{}.ndjson", safe_session_id));

    Ok(file_path.to_string_lossy().to_string())
}

/// List all research log files.
#[tauri::command]
pub fn list_research_logs() -> Result<Vec<String>, String> {
    let log_dir = research_logs_dir()?;

    if !log_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&log_dir)
        .map_err(|e| format!("Failed to read log directory: {}", e))?;

    let mut files = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".ndjson") {
                files.push(name.to_string());
            }
        }
    }

    files.sort();
    Ok(files)
}
