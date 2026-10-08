#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

mod capabilities;
mod error;
#[cfg(any(test, feature = "test-utils"))]
mod fixture;
mod format;
mod parser;
mod reader;
mod role;

// =============================================================================
// Public API: Parser + Core Re-exports (minimal surface)
// =============================================================================

/// The GGUF parser implementation.
pub use parser::GgufParser;

// Re-export domain types and port from core for convenience
pub use gglib_core::domain::gguf::GgufValue;
pub use gglib_core::{GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort};

// Re-export tool support detector
pub use capabilities::tool_calling::ToolSupportDetector;

// A GGUF file written without a model, for another crate's tests.
#[cfg(any(test, feature = "test-utils"))]
pub use fixture::write_string_gguf;
