//! GGUF format constants and definitions.
//!
//! This module contains magic numbers, type codes, and other format
//! constants for the GGUF binary format.

/// GGUF magic number (4 bytes): "GGUF" in little-endian.
pub(crate) const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Architecture-specific context length metadata keys.
///
/// Ordered by popularity for optimized lookup.
pub(crate) const CONTEXT_LENGTH_KEYS: [&str; 9] = [
    "llama.context_length",
    "qwen2.context_length",
    "qwen3.context_length",
    "mistral.context_length",
    "gpt2.context_length",
    "bert.context_length",
    "qwen.context_length",
    "qwen3moe.context_length",
    "context_length",
];
