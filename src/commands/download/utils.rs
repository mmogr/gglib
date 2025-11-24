use crate::utils::paths::{DirectoryCreationStrategy, ensure_directory, resolve_models_dir};
use anyhow::Result;
use std::path::PathBuf;

/// Sanitize model name for use as directory name
///
/// Converts potentially problematic characters in model names to safe
/// alternatives for use in file system paths.
///
/// # Arguments
///
/// * `name` - The model name to sanitize
///
/// # Returns
///
/// Returns a sanitized string suitable for directory names.
///
/// # Examples
///
/// ```rust
/// use gglib::commands::download::sanitize_model_name;
///
/// assert_eq!(sanitize_model_name("microsoft/DialoGPT-medium"), "microsoft_DialoGPT-medium");
/// assert_eq!(sanitize_model_name("path\\with:colons"), "path_with_colons");
/// ```
pub fn sanitize_model_name(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}

/// Resolve the models directory, honoring environment overrides and defaults.
pub fn get_models_directory() -> Result<PathBuf> {
    let resolution = resolve_models_dir(None)?;
    ensure_directory(&resolution.path, DirectoryCreationStrategy::AutoCreate)?;
    Ok(resolution.path)
}

/// Format large numbers for display
pub fn format_number(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}
