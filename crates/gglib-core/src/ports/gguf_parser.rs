//! GGUF parser port definition.
//!
//! This port abstracts the parsing of GGUF file metadata, allowing
//! different implementations (full parser, stub, mock for testing).
//!
//! # Design
//!
//! - Domain types (`GgufMetadata`, `GgufCapabilities`) are defined in `domain::gguf`
//! - This port only defines the trait and error type
//! - Implementations live in `gglib-gguf` crate

use std::path::Path;

use thiserror::Error;

// Re-export domain types for convenience
pub use crate::domain::gguf::{GgufCapabilities, GgufMetadata};
pub use crate::domain::tensor_table::TensorTable;

/// Errors that can occur during GGUF parsing.
///
/// This is the domain-facing error type. Implementations may have richer
/// internal errors that convert to this type via `From`.
#[derive(Debug, Error)]
pub enum GgufParseError {
    /// The file does not exist.
    #[error("File not found: {0}")]
    NotFound(String),

    /// The file is not a valid GGUF file.
    #[error("Invalid GGUF format: {0}")]
    InvalidFormat(String),

    /// IO error while reading the file.
    #[error("IO error: {0}")]
    Io(String),
}

/// Port for parsing GGUF file metadata.
///
/// This trait abstracts GGUF parsing so that different implementations
/// can be injected (full native parser, stub for tests, etc.).
///
/// # Port Signature Rules
///
/// - All types in signatures are from `gglib-core` (domain types)
/// - No `gglib-gguf` symbols appear in signatures
/// - Implementations live in `gglib-gguf` and implement this trait
pub trait GgufParserPort: Send + Sync {
    /// Parse metadata from a GGUF file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the GGUF file
    ///
    /// # Returns
    ///
    /// Returns the parsed metadata, or an error if parsing fails.
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError>;

    /// Detect capabilities from parsed metadata.
    ///
    /// Analyzes the metadata to detect model capabilities like reasoning
    /// or tool calling support.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The parsed GGUF metadata
    ///
    /// # Returns
    ///
    /// Returns structured capabilities (bitflags + extensions).
    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities;

    /// Read the tensor table of the GGUF or safetensors file at `path`.
    ///
    /// The first four bytes choose the format: the GGUF magic, or else a
    /// safetensors header length. Only the header is read, never a tensor's
    /// data, and no size the file declares is trusted before it is held to
    /// what the file can hold.
    ///
    /// # Errors
    ///
    /// When the file cannot be read, or its header is not a well-formed
    /// GGUF or safetensors header.
    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError>;

    /// Read the tensor table from `head`, the first bytes of a weights file,
    /// as [`Self::tensor_table`] reads it from the whole file.
    ///
    /// # Errors
    ///
    /// As [`Self::tensor_table`], and when the header runs past the end of
    /// `head`.
    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError>;
}

/// A no-op GGUF parser that returns default/empty metadata.
///
/// Useful for testing or when GGUF parsing is not needed.
#[derive(Debug, Clone, Default)]
pub struct NoopGgufParser;

impl GgufParserPort for NoopGgufParser {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Ok(GgufMetadata::default())
    }

    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }

    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        Ok(empty_table())
    }

    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Ok(empty_table())
    }
}

/// A GGUF table of no tensors, which is what [`NoopGgufParser`] reads.
const fn empty_table() -> TensorTable {
    TensorTable {
        format: crate::domain::tensor_table::WeightsFormat::Gguf,
        architecture: None,
        tensors: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_noop_parser_returns_empty() {
        let parser = NoopGgufParser;
        let result = parser.parse(&PathBuf::from("/any/path.gguf"));
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.architecture.is_none());
        assert!(meta.param_count_b.is_none());
    }

    #[test]
    fn test_noop_parser_detects_no_capabilities() {
        let parser = NoopGgufParser;
        let metadata = GgufMetadata::default();
        let caps = parser.detect_capabilities(&metadata);
        assert!(!caps.has_reasoning());
        assert!(!caps.has_tool_calling());
        assert!(caps.to_tags().is_empty());
    }
}
