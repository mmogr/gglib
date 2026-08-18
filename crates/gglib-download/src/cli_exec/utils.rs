//! Utility functions for CLI download operations.

use std::path::{Path, PathBuf};

/// Sanitize model name for use as directory name.
///
/// Converts potentially problematic characters in model names to safe
/// alternatives for use in file system paths.
pub(crate) fn sanitize_model_name(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}

/// Build the model directory path from `models_dir` and `model_id`.
pub(crate) fn model_directory(models_dir: &Path, model_id: &str) -> PathBuf {
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
}
