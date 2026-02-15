#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

// Silence unused dependency warnings for optional/future use
#[cfg(feature = "mmap")]
use memmap2 as _;
use tracing as _;

mod capabilities;
mod error;
mod format;
mod parser;
mod reader;
mod validation;

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

// Re-export validation primitives
pub use validation::{compute_gguf_sha256, validate_gguf_quick, ValidationError};
