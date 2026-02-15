//! Pure GGUF file validation primitives
//!
//! This module provides low-level validation functions for GGUF files that are
//! independent of business logic. These primitives can be used by higher-level
//! services to implement verification workflows.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// GGUF magic bytes: "GGUF" (version 3)
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Quick validation: checks GGUF magic bytes and file size
///
/// This is a fast check suitable for download verification.
/// Does not compute checksums.
///
/// # Arguments
///
/// * `path` - Path to the GGUF file to validate
/// * `expected_size` - Optional expected file size in bytes
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use gglib_gguf::validate_gguf_quick;
///
/// let result = validate_gguf_quick(Path::new("model.gguf"), Some(4368438272));
/// assert!(result.is_ok());
/// ```
pub fn validate_gguf_quick(path: &Path, expected_size: Option<u64>) -> Result<(), ValidationError> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| ValidationError::IoError(format!("cannot stat file: {e}")))?;

    let actual_size = metadata.len();

    // Check size if provided
    if let Some(expected) = expected_size {
        if actual_size != expected {
            return Err(ValidationError::SizeMismatch {
                expected,
                actual: actual_size,
            });
        }
    }

    // Check GGUF magic (read only first 4 bytes)
    let mut file =
        File::open(path).map_err(|e| ValidationError::IoError(format!("cannot open file: {e}")))?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| ValidationError::IoError(format!("cannot read magic bytes: {e}")))?;

    if magic != GGUF_MAGIC {
        return Err(ValidationError::InvalidMagic {
            expected: GGUF_MAGIC,
            actual: magic,
        });
    }

    Ok(())
}

/// Compute SHA256 hash of a file with progress reporting
///
/// Streams the file in chunks to avoid memory issues with large files.
/// The progress callback receives (`bytes_processed`, `total_bytes`).
///
/// This function is designed to be called from `tokio::task::spawn_blocking`
/// as it performs blocking I/O.
///
/// # Arguments
///
/// * `path` - Path to the file to hash
/// * `progress_callback` - Callback function invoked periodically with progress updates
///
/// # Returns
///
/// Returns the SHA256 hash as a lowercase hexadecimal string
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use gglib_gguf::compute_gguf_sha256;
///
/// let hash = compute_gguf_sha256(
///     Path::new("model.gguf"),
///     |processed, total| {
///         let percent = (processed as f64 / total as f64 * 100.0) as u8;
///         println!("Progress: {}%", percent);
///     }
/// ).unwrap();
/// println!("SHA256: {}", hash);
/// ```
pub fn compute_gguf_sha256<F>(
    path: &Path,
    mut progress_callback: F,
) -> Result<String, ValidationError>
where
    F: FnMut(u64, u64),
{
    let mut file =
        File::open(path).map_err(|e| ValidationError::IoError(format!("cannot open file: {e}")))?;

    let total_bytes = file
        .metadata()
        .map_err(|e| ValidationError::IoError(format!("cannot get file size: {e}")))?
        .len();

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks
    let mut bytes_processed = 0u64;

    // Initial callback
    progress_callback(0, total_bytes);

    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| ValidationError::IoError(format!("read error: {e}")))?;

        if n == 0 {
            break;
        }

        hasher.update(&buffer[..n]);
        bytes_processed += n as u64;

        // Report progress every ~100MB or at end
        if bytes_processed % (100 * 1024 * 1024) < (1024 * 1024) || bytes_processed == total_bytes {
            progress_callback(bytes_processed, total_bytes);
        }
    }

    // Final callback
    progress_callback(total_bytes, total_bytes);

    Ok(format!("{:x}", hasher.finalize()))
}

/// Validation errors - pure infrastructure concerns only
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("I/O error: {0}")]
    IoError(String),

    #[error("Size mismatch: expected {expected} bytes, got {actual} bytes")]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("Invalid GGUF magic bytes: expected {expected:?}, got {actual:?}")]
    InvalidMagic { expected: [u8; 4], actual: [u8; 4] },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_sha256() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"test data").unwrap();
        file.flush().unwrap();

        let mut progress_calls = 0;
        let hash = compute_gguf_sha256(file.path(), |_, _| {
            progress_calls += 1;
        })
        .unwrap();

        // Known SHA256 of "test data"
        assert_eq!(
            hash,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
        assert!(progress_calls >= 2); // At least start and end
    }

    #[test]
    fn test_validate_quick_invalid_magic() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"FAKE").unwrap();
        file.flush().unwrap();

        let result = validate_gguf_quick(file.path(), None);
        assert!(matches!(result, Err(ValidationError::InvalidMagic { .. })));
    }

    #[test]
    fn test_validate_quick_size_mismatch() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x47, 0x47, 0x55, 0x46]).unwrap(); // GGUF magic
        file.write_all(&[0u8; 100]).unwrap();
        file.flush().unwrap();

        let result = validate_gguf_quick(file.path(), Some(50));
        assert!(matches!(result, Err(ValidationError::SizeMismatch { .. })));
    }

    #[test]
    fn test_validate_quick_valid() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x47, 0x47, 0x55, 0x46]).unwrap(); // GGUF magic
        file.write_all(&[0u8; 100]).unwrap();
        file.flush().unwrap();

        let result = validate_gguf_quick(file.path(), Some(104));
        assert!(result.is_ok());
    }

    #[test]
    fn test_compute_sha256_with_progress_updates() {
        let mut file = NamedTempFile::new().unwrap();
        // Write enough data to trigger multiple progress updates
        let data = vec![0u8; 5 * 1024 * 1024]; // 5MB
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let mut progress_updates = Vec::new();
        let _hash = compute_gguf_sha256(file.path(), |processed, total| {
            progress_updates.push((processed, total));
        })
        .unwrap();

        // Should have at least initial and final progress
        assert!(progress_updates.len() >= 2);
        // First update should be 0 bytes
        assert_eq!(progress_updates[0].0, 0);
        // Last update should be total bytes
        assert_eq!(
            progress_updates.last().unwrap().0,
            progress_updates.last().unwrap().1
        );
    }
}
