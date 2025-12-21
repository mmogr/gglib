//! GGUF format constants and definitions.
//!
//! This module contains magic numbers, type codes, and other format
//! constants for the GGUF binary format.

/// GGUF magic number (4 bytes): "GGUF" in little-endian.
pub const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Known quantization types and their file type codes.
pub mod quantization {
    /// Map GGUF file type number to quantization string.
    #[must_use]
    pub const fn from_file_type(file_type: u32) -> Option<&'static str> {
        match file_type {
            0 => Some("F32"),
            1 => Some("F16"),
            2 => Some("Q4_0"),
            3 => Some("Q4_1"),
            6 => Some("Q5_0"),
            7 => Some("Q5_1"),
            8 => Some("Q8_0"),
            _ => None,
        }
    }

    /// Known quantization patterns to search for in filenames.
    /// Ordered with longer patterns first to avoid partial matches.
    pub const KNOWN_PATTERNS: [&str; 14] = [
        "Q8_K_XL", "Q8_K_L", "Q8_K_M", "Q4_K_M", "Q4_K_S", "Q5_K_M", "Q5_K_S", "Q8_K", "Q6_K",
        "Q3_K", "Q2_K", "Q8_0", "F16", "BF16",
    ];
}

/// Architecture-specific context length metadata keys.
///
/// Ordered by popularity for optimized lookup.
pub const CONTEXT_LENGTH_KEYS: [&str; 9] = [
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
