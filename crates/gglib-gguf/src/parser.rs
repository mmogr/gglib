//! GGUF parser implementation.
//!
//! This module provides the main `GgufParser` struct that implements
//! the `GgufParserPort` trait from `gglib-core`.

use std::collections::HashMap;
use std::path::Path;

use gglib_core::domain::gguf::{GgufValue, RawMetadata};
use gglib_core::{GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort};

use crate::capabilities;
use crate::error::GgufResult;
use crate::format::{CONTEXT_LENGTH_KEYS, quantization};
use crate::reader::GgufReader;

/// GGUF file parser.
///
/// Implements `GgufParserPort` from `gglib-core`, providing full GGUF
/// parsing and capability detection functionality.
#[derive(Debug, Clone, Default)]
pub struct GgufParser;

impl GgufParser {
    /// Create a new GGUF parser.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Internal parse implementation that returns rich internal errors.
    #[allow(clippy::unused_self)]
    fn parse_internal(&self, file_path: &Path) -> GgufResult<GgufMetadata> {
        let mut reader = GgufReader::open(file_path)?;

        // Read and validate header
        reader.read_magic()?;
        let version = reader.read_version()?;

        // Read tensor count (not used but must be read)
        let _tensor_count = if version >= 2 {
            reader.read_u64()?
        } else {
            u64::from(reader.read_u32()?)
        };

        // Read metadata count
        let metadata_count = if version >= 2 {
            reader.read_u64()?
        } else {
            u64::from(reader.read_u32()?)
        };

        // Parse metadata key-value pairs
        let mut raw_metadata = HashMap::new();
        for _ in 0..metadata_count {
            let key = reader.read_string()?;
            let value_type = reader.read_u32()?;
            let value = reader.read_value(value_type)?;
            raw_metadata.insert(key, value);
        }

        // Extract structured metadata
        Ok(extract_metadata(&raw_metadata, file_path))
    }
}

impl GgufParserPort for GgufParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        self.parse_internal(file_path).map_err(Into::into)
    }

    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities {
        capabilities::detect_all(&metadata.metadata)
    }
}

// =============================================================================
// Metadata Extraction
// =============================================================================

/// Extract structured metadata from raw GGUF key-value pairs.
fn extract_metadata(raw: &RawMetadata, file_path: &Path) -> GgufMetadata {
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
    let context_length = extract_context_length(raw, architecture.as_ref());
    let param_count_b = extract_param_count(raw, file_path);
    let quantization = extract_quantization(raw, file_path);
    
    // Extract MoE metadata
    let (expert_count, expert_used_count, expert_shared_count) = 
        extract_moe_metadata(raw, architecture.as_ref());

    GgufMetadata {
        name,
        architecture,
        param_count_b,
        quantization,
        context_length,
        expert_count,
        expert_used_count,
        expert_shared_count,
        metadata: processed,
    }
}

/// Extract model name from metadata or filename.
fn extract_name(raw: &RawMetadata, file_path: &Path) -> Option<String> {
    raw.get("general.name")
        .map(std::string::ToString::to_string)
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(std::string::ToString::to_string)
        })
}

/// Extract model architecture from metadata.
fn extract_architecture(raw: &RawMetadata) -> Option<String> {
    raw.get("general.architecture")
        .map(std::string::ToString::to_string)
}

/// Extract MoE (Mixture-of-Experts) metadata from architecture-specific keys.
fn extract_moe_metadata(
    raw: &RawMetadata,
    architecture: Option<&String>,
) -> (Option<u32>, Option<u32>, Option<u32>) {
    let arch = match architecture {
        Some(a) => a,
        None => return (None, None, None),
    };

    let expert_count = raw
        .get(&format!("{}.expert_count", arch))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let expert_used_count = raw
        .get(&format!("{}.expert_used_count", arch))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let expert_shared_count = raw
        .get(&format!("{}.expert_shared_count", arch))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    (expert_count, expert_used_count, expert_shared_count)
}

/// Extract context length from architecture-specific metadata.
fn extract_context_length(raw: &RawMetadata, architecture: Option<&String>) -> Option<u64> {
    // Try architecture-specific key first (e.g., "llama.context_length")
    if let Some(arch) = architecture {
        let arch_key = format!("{}.context_length", arch);
        if let Some(value) = raw.get(&arch_key) {
            if let Some(length) = value.as_u64() {
                return Some(length);
            }
        }
    }

    // Fallback to generic key
    if let Some(value) = raw.get("context_length") {
        if let Some(length) = value.as_u64() {
            return Some(length);
        }
    }

    // Legacy: Try hardcoded common keys as last resort
    for key in CONTEXT_LENGTH_KEYS {
        if let Some(value) = raw.get(key) {
            if let Some(length) = value.as_u64() {
                return Some(length);
            }
        }
    }

    None
}

/// Extract parameter count from metadata or filename.
fn extract_param_count(raw: &RawMetadata, file_path: &Path) -> Option<f64> {
    // Priority #1: Check for standard numeric parameter_count key
    if let Some(value) = raw.get("general.parameter_count") {
        if let Some(count) = value.as_u64() {
            #[allow(clippy::cast_precision_loss)]
            return Some(count as f64 / 1_000_000_000.0);
        }
        if let Some(count) = value.as_f64() {
            return Some(count / 1_000_000_000.0);
        }
    }

    // Priority #2: Parse general.size_label
    if let Some(size_label) = raw.get("general.size_label") {
        if let Some(params) = parse_param_label(&size_label.to_string()) {
            return Some(params);
        }
    }

    // Priority #3: Fallback to filename parsing
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        if let Some(params) = parse_param_from_filename(filename) {
            return Some(params);
        }
    }

    None
}

/// Parse parameter count from size label (e.g., "7B", "13B", "70B", "8x7B").
/// For MoE models ("NxM.MB" format), returns TOTAL parameters (N × M) for VRAM estimation.
fn parse_param_label(size_label: &str) -> Option<f64> {
    let upper = size_label.to_uppercase();

    // Handle MoE format: "8x7B", "64x2.6B", "512x2.5B" (NxM.MB)
    // Calculate TOTAL parameters: N × M (e.g., 64 × 2.6 = 166.4B)
    if let Some(x_pos) = upper.find('X') {
        let before_x = &upper[..x_pos];
        let after_x = &upper[x_pos + 1..];
        
        if let Some(expert_size_str) = after_x.strip_suffix('B') {
            if let (Ok(expert_count), Ok(expert_size)) = (
                before_x.parse::<f64>(),
                expert_size_str.parse::<f64>(),
            ) {
                // Return total: expert_count × expert_size
                return Some(expert_count * expert_size);
            }
        }
    }

    // Handle regular format: "7B", "13B", "70B"
    if upper.ends_with('B') {
        let number_part = &upper[..upper.len() - 1];
        if let Ok(num) = number_part.parse::<f64>() {
            return Some(num);
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

/// Extract quantization from filename.
fn extract_quantization_from_filename(filename: &str) -> String {
    let upper = filename.to_ascii_uppercase();

    for q in quantization::KNOWN_PATTERNS {
        if upper.contains(q) {
            return q.to_string();
        }
    }

    "Unknown".to_string()
}

/// Extract quantization from metadata or filename.
fn extract_quantization(raw: &RawMetadata, file_path: &Path) -> Option<String> {
    // First try filename parsing
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        let quant = extract_quantization_from_filename(filename);
        if quant != "Unknown" {
            return Some(quant);
        }
    }

    // Fallback to file_type metadata
    if let Some(file_type) = raw.get("general.file_type") {
        let type_str = file_type.to_string();
        if let Ok(type_num) = type_str.parse::<u32>() {
            if let Some(quant) = quantization::from_file_type(type_num) {
                return Some(quant.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_param_label() {
        // Regular models
        assert!((parse_param_label("7B").unwrap() - 7.0).abs() < f64::EPSILON);
        assert!((parse_param_label("13B").unwrap() - 13.0).abs() < f64::EPSILON);
        assert!((parse_param_label("70B").unwrap() - 70.0).abs() < f64::EPSILON);
        
        // MoE models - should return TOTAL parameters (N × M)
        assert!((parse_param_label("8x7B").unwrap() - 56.0).abs() < f64::EPSILON);
        assert!((parse_param_label("64x2.6B").unwrap() - 166.4).abs() < 0.01);
        assert!((parse_param_label("512x2.5B").unwrap() - 1280.0).abs() < 0.01);
        
        // Invalid
        assert!(parse_param_label("invalid").is_none());
    }

    #[test]
    fn test_parse_param_from_filename() {
        assert!(
            (parse_param_from_filename("llama-7b-chat.gguf").unwrap() - 7.0).abs() < f64::EPSILON
        );
        assert!(
            (parse_param_from_filename("model-13B-q4.gguf").unwrap() - 13.0).abs() < f64::EPSILON
        );
        assert!(parse_param_from_filename("no-params.gguf").is_none());
    }

    #[test]
    fn test_extract_context_length_dynamic() {
        // Test with architecture-specific key
        let mut raw = HashMap::new();
        raw.insert("llama.context_length".to_string(), GgufValue::U32(4096));
        let arch = Some("llama".to_string());
        assert_eq!(extract_context_length(&raw, arch.as_ref()), Some(4096));

        // Test with different architecture
        let mut raw2 = HashMap::new();
        raw2.insert("qwen2.context_length".to_string(), GgufValue::U64(32768));
        let arch2 = Some("qwen2".to_string());
        assert_eq!(extract_context_length(&raw2, arch2.as_ref()), Some(32768));
        
        // Test with new architectures (deepseek2, qwen3next)
        let mut raw3 = HashMap::new();
        raw3.insert("deepseek2.context_length".to_string(), GgufValue::U64(131072));
        let arch3 = Some("deepseek2".to_string());
        assert_eq!(extract_context_length(&raw3, arch3.as_ref()), Some(131072));
        
        let mut raw4 = HashMap::new();
        raw4.insert("qwen3next.context_length".to_string(), GgufValue::U32(131072));
        let arch4 = Some("qwen3next".to_string());
        assert_eq!(extract_context_length(&raw4, arch4.as_ref()), Some(131072));
        
        // Test generic fallback
        let mut raw_generic = HashMap::new();
        raw_generic.insert("context_length".to_string(), GgufValue::U32(2048));
        assert_eq!(extract_context_length(&raw_generic, None), Some(2048));
    }

    #[test]
    fn test_extract_moe_metadata() {
        let mut raw = HashMap::new();
        raw.insert("deepseek2.expert_count".to_string(), GgufValue::U32(64));
        raw.insert("deepseek2.expert_used_count".to_string(), GgufValue::U32(4));
        raw.insert("deepseek2.expert_shared_count".to_string(), GgufValue::U32(1));
        
        let arch = Some("deepseek2".to_string());
        let (expert_count, expert_used_count, expert_shared_count) = 
            extract_moe_metadata(&raw, arch.as_ref());
        
        assert_eq!(expert_count, Some(64));
        assert_eq!(expert_used_count, Some(4));
        assert_eq!(expert_shared_count, Some(1));
        
        // Test with no architecture
        let (ec, euc, esc) = extract_moe_metadata(&raw, None);
        assert_eq!(ec, None);
        assert_eq!(euc, None);
        assert_eq!(esc, None);
    }

    #[test]
    fn test_extract_quantization_from_filename() {
        assert_eq!(
            extract_quantization_from_filename("model-Q4_K_M.gguf"),
            "Q4_K_M"
        );
        assert_eq!(extract_quantization_from_filename("model-f16.gguf"), "F16");
        assert_eq!(extract_quantization_from_filename("model.gguf"), "Unknown");
    }
}
