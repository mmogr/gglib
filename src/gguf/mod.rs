//! GGUF file parsing and model capability detection.
//!
//! This module provides a complete implementation for parsing GGUF (GGML Universal Format)
//! files and extracting metadata to support model management, serving, and capability detection.
//!
//! # Modules
//!
//! - [`error`] - Error types for GGUF parsing
//! - [`types`] - Core types: `GgufValue`, `GgufMetadata`
//! - [`reader`] - Low-level binary readers
//! - [`metadata`] - Metadata extraction and parsing
//! - [`capabilities`] - Reasoning and tool calling detection
//!
//! # Quick Start
//!
//! ```no_run
//! use gglib::gguf::{parse_gguf_file, apply_capability_detection};
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Parse a GGUF file
//! let path = Path::new("model.gguf");
//! let metadata = parse_gguf_file(path)?;
//!
//! println!("Model: {}", metadata.name.as_deref().unwrap_or("unknown"));
//! println!("Parameters: {:?}", metadata.param_count_b);
//! println!("Quantization: {:?}", metadata.quantization);
//!
//! // Detect capabilities
//! let tags = apply_capability_detection(&metadata.metadata);
//! for tag in tags {
//!     println!("Auto-detected tag: {}", tag);
//! }
//! # Ok(())
//! # }
//! ```

mod capabilities;
mod error;
mod metadata;
mod reader;
mod types;

// Re-export error types
pub use error::{GgufError, GgufResult};

// Re-export core types
pub use types::{GgufMetadata, GgufValue};

// Re-export metadata parsing
pub use metadata::{extract_metadata, parse_gguf_file};

// Re-export capability detection
pub use capabilities::{
    ReasoningDetection, ToolCallingDetection, apply_capability_detection,
    apply_reasoning_detection, apply_tool_detection, detect_reasoning_support, detect_tool_support,
    is_reasoning_model, is_tool_capable_model,
};

// Re-export constants that may be useful externally
pub use reader::GGUF_MAGIC;
