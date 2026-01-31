//! Application log commands.
//!
//! Commands for bridging frontend logs to Rust tracing infrastructure.
//! Frontend logs are mapped to tracing events with the target "gglib.frontend.*".
use serde::Deserialize;

/// Frontend log entry structure.
///
/// Matches the LogEntry interface in TypeScript.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // timestamp is required for deserialization but not used in logging
pub struct FrontendLogEntry {
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub message: String,
    #[serde(default)]
    pub data: Option<String>, // JSON string
}

/// Bridge frontend logs into Rust tracing.
///
/// This command receives log entries from the TypeScript frontend via Tauri IPC
/// and re-emits them as tracing events. The logs are:
/// - Written to stdout via tracing_subscriber
/// - Written to files via tracing-appender
/// - Filtered by RUST_LOG environment variable
///
/// # Target Naming
///
/// All frontend logs use the static target `gglib_frontend` to satisfy tracing's
/// compile-time constant requirement. The category is included as a structured field.
///
/// # Example RUST_LOG filters
///
/// ```bash
/// # Show all frontend logs
/// RUST_LOG=gglib_frontend=debug
///
/// # Show frontend + backend
/// RUST_LOG=gglib=debug,gglib_frontend=debug
/// ```
#[tauri::command]
pub fn log_from_frontend(entry: FrontendLogEntry) -> Result<(), String> {
    let message = &entry.message;
    let category = &entry.category;

    // Log with static target and category as structured field
    if let Some(data) = &entry.data {
        match entry.level.as_str() {
            "debug" => {
                tracing::debug!(target: "gglib_frontend", category = %category, data = %data, "{}", message)
            }
            "info" => {
                tracing::info!(target: "gglib_frontend", category = %category, data = %data, "{}", message)
            }
            "warn" => {
                tracing::warn!(target: "gglib_frontend", category = %category, data = %data, "{}", message)
            }
            "error" => {
                tracing::error!(target: "gglib_frontend", category = %category, data = %data, "{}", message)
            }
            _ => {
                tracing::info!(target: "gglib_frontend", category = %category, data = %data, "{}", message)
            }
        }
    } else {
        match entry.level.as_str() {
            "debug" => {
                tracing::debug!(target: "gglib_frontend", category = %category, "{}", message)
            }
            "info" => tracing::info!(target: "gglib_frontend", category = %category, "{}", message),
            "warn" => tracing::warn!(target: "gglib_frontend", category = %category, "{}", message),
            "error" => {
                tracing::error!(target: "gglib_frontend", category = %category, "{}", message)
            }
            _ => tracing::info!(target: "gglib_frontend", category = %category, "{}", message),
        }
    }

    Ok(())
}
