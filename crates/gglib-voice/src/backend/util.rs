//! Shared utilities for voice backend implementations.

use std::path::Path;

/// Convert a `Path` to a `String`, mapping invalid UTF-8 to a caller-supplied error.
///
/// Each backend uses a different [`VoiceError`](crate::error::VoiceError) variant for
/// path errors (e.g. `ModelLoadError` for STT, `SynthesisError` for TTS). The `err`
/// closure receives a human-readable description and should wrap it in the appropriate
/// variant.
///
/// # Example
/// ```ignore
/// util::path_to_string(&path, VoiceError::ModelLoadError)?;
/// util::path_to_string(&path, VoiceError::SynthesisError)?;
/// ```
pub(super) fn path_to_string<E>(path: &Path, err: impl FnOnce(String) -> E) -> Result<String, E> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| err(format!("Invalid UTF-8 path: {}", path.display())))
}
