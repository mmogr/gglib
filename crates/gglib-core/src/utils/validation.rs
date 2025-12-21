//! File validation utilities for GGUF model files.
//!
//! This module provides validation functions to ensure files are
//! valid GGUF format and can be safely processed by the parser.

use crate::ports::{GgufMetadata, GgufParserPort};
use anyhow::{Result, anyhow};
use std::path::Path;

/// Validates that a file exists and has a .gguf extension.
///
/// # Arguments
/// * `file_path` - Path to the file to validate
///
/// # Returns
/// * `Ok(())` if valid, `Err` if file doesn't exist or has wrong extension
///
/// # Examples
///
/// ```rust
/// use gglib_core::utils::validation::validate_file;
/// use std::fs::File;
/// use tempfile::tempdir;
///
/// // Create a temporary .gguf file
/// let temp_dir = tempdir().unwrap();
/// let file_path = temp_dir.path().join("model.gguf");
/// File::create(&file_path).unwrap();
///
/// // Validate the file
/// let result = validate_file(file_path.to_str().unwrap());
/// assert!(result.is_ok());
/// ```
///
/// ```rust
/// use gglib_core::utils::validation::validate_file;
///
/// // Non-existent file should fail
/// let result = validate_file("/nonexistent/model.gguf");
/// assert!(result.is_err());
/// ```
pub fn validate_file(file_path: &str) -> Result<()> {
    let path: &Path = Path::new(file_path);

    if !path.exists() {
        return Err(anyhow!("File does not exist: {file_path}"));
    }
    match path.extension() {
        Some(ext) if ext == "gguf" => Ok(()),
        Some(_) => Err(anyhow!("Wrong extension.")),
        None => Err(anyhow!("File has no extension.")),
    }
}

/// Validates a GGUF file and extracts its metadata using the provided parser.
///
/// This function performs both file validation (existence and extension) and
/// GGUF format parsing to extract model metadata.
///
/// # Arguments
/// * `parser` - The GGUF parser to use (injected via port)
/// * `file_path` - Path to the GGUF file to validate and parse
///
/// # Returns
/// * `Ok(GgufMetadata)` with extracted metadata if valid
/// * `Err` if file doesn't exist, has wrong extension, or can't be parsed
pub fn validate_and_parse_gguf(
    parser: &dyn GgufParserPort,
    file_path: &str,
) -> Result<GgufMetadata> {
    // First validate the file exists and has correct extension
    validate_file(file_path)?;

    // Then parse the GGUF metadata using the injected parser
    let path = Path::new(file_path);
    let metadata = parser.parse(path)?;

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_validate_file_success() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.gguf");
        File::create(&file_path).unwrap();

        let result = validate_file(file_path.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_not_exists() {
        let result = validate_file("/nonexistent/path/model.gguf");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("File does not exist")
        );
    }

    #[test]
    fn test_validate_file_wrong_extension() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = validate_file(file_path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Wrong extension"));
    }

    #[test]
    fn test_validate_file_no_extension() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test_no_ext");
        File::create(&file_path).unwrap();

        let result = validate_file(file_path.to_str().unwrap());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("File has no extension")
        );
    }

    // Note: validate_and_parse_gguf tests would require mock GGUF files
    // These are better suited for integration tests with actual GGUF samples
}
