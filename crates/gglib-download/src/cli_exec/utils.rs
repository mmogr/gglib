//! Utility functions for CLI download operations.

use std::path::{Path, PathBuf};

/// Sanitize model name for use as directory name.
///
/// Converts potentially problematic characters in model names to safe
/// alternatives for use in file system paths.
pub fn sanitize_model_name(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}

/// Format large numbers for display (1000 → "1.0K", 1000000 → "1.0M").
#[allow(clippy::cast_precision_loss)]
pub fn format_number(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

/// Build the model directory path from `models_dir` and `model_id`.
pub fn model_directory(models_dir: &Path, model_id: &str) -> PathBuf {
    models_dir.join(sanitize_model_name(model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_model_name() {
        assert_eq!(
            sanitize_model_name("microsoft/DialoGPT"),
            "microsoft_DialoGPT"
        );
        assert_eq!(sanitize_model_name("path\\with:colons"), "path_with_colons");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1_500), "1.5K");
        assert_eq!(format_number(1_500_000), "1.5M");
    }
}
