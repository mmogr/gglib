//! Metadata extraction and processing for GGUF files.
//!
//! This module handles extraction of structured metadata from raw GGUF key-value pairs,
//! including context length resolution, parameter count parsing, and quantization detection.

#![allow(clippy::collapsible_if)]

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::commands::download::extract_quantization_from_filename;
use crate::gguf::error::{GgufError, GgufResult};
use crate::gguf::reader::{read_magic, read_string, read_u32, read_u64, read_value, read_version};
use crate::gguf::types::{GgufMetadata, GgufValue, RawMetadata};

// =============================================================================
// File Parsing (I/O orchestration)
// =============================================================================

/// Parse GGUF metadata from a file.
///
/// This is the main entry point for parsing GGUF files. It reads the file,
/// validates the header, and extracts structured metadata.
///
/// # Arguments
/// * `file_path` - Path to the GGUF file to parse
///
/// # Returns
/// * `Ok(GgufMetadata)` with extracted model information
/// * `Err(GgufError)` if file cannot be read or is not a valid GGUF file
pub fn parse_gguf_file<P: AsRef<Path>>(file_path: P) -> GgufResult<GgufMetadata> {
    let path = file_path.as_ref();
    let file = File::open(path).map_err(GgufError::from)?;
    let mut reader = BufReader::new(file);

    // Read and validate header
    read_magic(&mut reader)?;
    let version = read_version(&mut reader)?;

    // Read tensor count (not used but must be read)
    let _tensor_count = if version >= 2 {
        read_u64(&mut reader)?
    } else {
        read_u32(&mut reader)? as u64
    };

    // Read metadata count
    let metadata_count = if version >= 2 {
        read_u64(&mut reader)?
    } else {
        read_u32(&mut reader)? as u64
    };

    // Parse metadata key-value pairs
    let mut raw_metadata = HashMap::new();
    for _ in 0..metadata_count {
        let key = read_string(&mut reader)?;
        let value_type = read_u32(&mut reader)?;
        let value = read_value(&mut reader, value_type)?;
        raw_metadata.insert(key, value);
    }

    // Extract structured metadata
    extract_metadata(&raw_metadata, path)
}

// =============================================================================
// Metadata Extraction (Pure logic)
// =============================================================================

/// Extract structured metadata from raw GGUF key-value pairs.
///
/// This is the main processing function that converts raw metadata into
/// a structured `GgufMetadata` instance.
pub fn extract_metadata(raw: &RawMetadata, file_path: &Path) -> GgufResult<GgufMetadata> {
    let mut processed = HashMap::new();

    // Convert metadata to string representation, skipping large arrays
    for (key, value) in raw {
        if key.starts_with("tokenizer.")
            && matches!(value, GgufValue::Array(arr) if arr.len() > 100)
        {
            // Store just a summary for large tokenizer arrays
            if let GgufValue::Array(arr) = value {
                processed.insert(key.clone(), format!("Array with {} elements", arr.len()));
            }
        } else {
            processed.insert(key.clone(), value.to_string());
        }
    }

    // Extract fields
    let name = extract_name(raw, file_path);
    let architecture = extract_architecture(raw);
    let context_length = extract_context_length(raw);
    let param_count_b = extract_param_count(raw, file_path);
    let quantization = extract_quantization(raw, file_path);

    Ok(GgufMetadata {
        name,
        architecture,
        param_count_b,
        quantization,
        context_length,
        metadata: processed,
    })
}

/// Extract model name from metadata or filename.
fn extract_name(raw: &RawMetadata, file_path: &Path) -> Option<String> {
    raw.get("general.name").map(|v| v.to_string()).or_else(|| {
        file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    })
}

/// Extract model architecture from metadata.
fn extract_architecture(raw: &RawMetadata) -> Option<String> {
    raw.get("general.architecture").map(|v| v.to_string())
}

/// Architecture-specific context length keys.
///
/// Ordered by popularity to optimize lookup.
const CONTEXT_LENGTH_KEYS: &[&str] = &[
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

/// Extract context length from architecture-specific metadata.
fn extract_context_length(raw: &RawMetadata) -> Option<u64> {
    for key in CONTEXT_LENGTH_KEYS {
        if let Some(value) = raw.get(*key) {
            if let Some(length) = value.as_u64() {
                return Some(length);
            }
        }
    }
    None
}

/// Extract parameter count from metadata or filename.
fn extract_param_count(raw: &RawMetadata, file_path: &Path) -> Option<f64> {
    // Try metadata first
    if let Some(size_label) = raw.get("general.size_label") {
        if let Some(params) = parse_param_label(&size_label.to_string()) {
            return Some(params);
        }
    }

    // Fallback to filename parsing
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        if let Some(params) = parse_param_from_filename(filename) {
            return Some(params);
        }
    }

    None
}

/// Parse parameter count from size label (e.g., "7B", "13B", "70B", "8x7B").
fn parse_param_label(size_label: &str) -> Option<f64> {
    let upper = size_label.to_uppercase();

    // Handle "7B", "13B", "70B"
    if upper.ends_with('B') {
        let number_part = &upper[..upper.len() - 1];
        if let Ok(num) = number_part.parse::<f64>() {
            return Some(num);
        }
    }

    // Handle "8x7B" (mixture of experts)
    if let Some(x_pos) = upper.find('X') {
        let after_x = &upper[x_pos + 1..];
        if let Some(number_part) = after_x.strip_suffix('B') {
            if let Ok(num) = number_part.parse::<f64>() {
                return Some(num);
            }
        }
    }

    None
}

/// Parse parameter count from filename patterns.
fn parse_param_from_filename(filename: &str) -> Option<f64> {
    let upper = filename.to_uppercase();

    for pattern in &["B", "BILLION"] {
        if let Some(pos) = upper.find(pattern) {
            let before = &upper[..pos];

            // Find the last numeric sequence (possibly with decimal)
            let mut number_str = String::new();
            let mut found_digit = false;

            for ch in before.chars().rev() {
                if ch.is_ascii_digit() || ch == '.' {
                    number_str.insert(0, ch);
                    found_digit = true;
                } else if found_digit {
                    break;
                }
            }

            if let Ok(num) = number_str.parse::<f64>() {
                return Some(num);
            }
        }
    }

    None
}

/// Extract quantization from metadata or filename.
fn extract_quantization(raw: &RawMetadata, file_path: &Path) -> Option<String> {
    // First try filename parsing using the canonical Quantization enum
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        let quant = extract_quantization_from_filename(filename);
        if !quant.is_unknown() {
            return Some(quant.to_string());
        }
    }

    // Fallback to file_type metadata
    if let Some(file_type) = raw.get("general.file_type") {
        if let Some(quant) = map_file_type_to_quantization(&file_type.to_string()) {
            return Some(quant);
        }
    }

    None
}

/// Map GGUF file type number to quantization string.
fn map_file_type_to_quantization(file_type: &str) -> Option<String> {
    match file_type {
        "0" => Some("F32".to_string()),
        "1" => Some("F16".to_string()),
        "2" => Some("Q4_0".to_string()),
        "3" => Some("Q4_1".to_string()),
        "6" => Some("Q5_0".to_string()),
        "7" => Some("Q5_1".to_string()),
        "8" => Some("Q8_0".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_param_label() {
        assert_eq!(parse_param_label("7B"), Some(7.0));
        assert_eq!(parse_param_label("13B"), Some(13.0));
        assert_eq!(parse_param_label("70B"), Some(70.0));
        assert_eq!(parse_param_label("8x7B"), Some(7.0));
        assert_eq!(parse_param_label("invalid"), None);
    }

    #[test]
    fn test_parse_param_from_filename() {
        assert_eq!(parse_param_from_filename("llama-7b-chat.gguf"), Some(7.0));
        assert_eq!(parse_param_from_filename("model-13B-q4.gguf"), Some(13.0));
        assert_eq!(parse_param_from_filename("no-params.gguf"), None);
    }

    #[test]
    fn test_extract_context_length() {
        let mut raw = HashMap::new();
        raw.insert("llama.context_length".to_string(), GgufValue::U32(4096));
        assert_eq!(extract_context_length(&raw), Some(4096));

        let mut raw2 = HashMap::new();
        raw2.insert("qwen2.context_length".to_string(), GgufValue::U64(32768));
        assert_eq!(extract_context_length(&raw2), Some(32768));
    }

    #[test]
    fn test_map_file_type_to_quantization() {
        assert_eq!(map_file_type_to_quantization("0"), Some("F32".to_string()));
        assert_eq!(map_file_type_to_quantization("1"), Some("F16".to_string()));
        assert_eq!(map_file_type_to_quantization("2"), Some("Q4_0".to_string()));
        assert_eq!(map_file_type_to_quantization("99"), None);
    }
}
