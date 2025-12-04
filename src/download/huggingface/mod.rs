//! HuggingFace file resolution.

pub mod file_resolver;

pub use file_resolver::{FileResolution, QuantizationFileResolver, resolve_quantization_files};
