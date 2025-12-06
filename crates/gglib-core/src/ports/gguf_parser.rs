//! GGUF parser port definition.
//!
//! This port abstracts the parsing of GGUF file metadata, allowing
//! different implementations (full parser, stub, mock for testing).

use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

/// Errors that can occur during GGUF parsing.
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

/// Parsed metadata from a GGUF file.
#[derive(Debug, Clone, Default)]
pub struct GgufMetadata {
    /// Model architecture (e.g., "llama", "mistral").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "Q4_K_M", "Q8_0").
    pub quantization: Option<String>,
    /// Number of parameters in billions.
    pub param_count_b: Option<f64>,
    /// Maximum context length.
    pub context_length: Option<u64>,
    /// Additional key-value metadata from the file.
    pub metadata: HashMap<String, String>,
}

/// Port for parsing GGUF file metadata.
///
/// This trait abstracts GGUF parsing so that different implementations
/// can be injected (full native parser, stub for tests, etc.).
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

    /// Detect capability tags from metadata.
    ///
    /// Analyzes the metadata to detect model capabilities like reasoning
    /// or tool calling support.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The metadata key-value pairs
    ///
    /// # Returns
    ///
    /// Returns a list of detected capability tags.
    fn detect_capabilities(&self, metadata: &HashMap<String, String>) -> Vec<String>;
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

    fn detect_capabilities(&self, _metadata: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
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
        let caps = parser.detect_capabilities(&HashMap::new());
        assert!(caps.is_empty());
    }
}
