//! HuggingFace file resolution.

pub mod file_resolver;

pub use file_resolver::{resolve_quantization_files, FileResolution, QuantizationFileResolver};