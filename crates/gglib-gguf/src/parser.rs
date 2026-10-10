//! GGUF parser implementation.
//!
//! This module provides the main `GgufParser` struct that implements
//! the `GgufParserPort` trait from `gglib-core`.

use std::collections::HashMap;
use std::path::Path;

use gglib_core::domain::gguf::{GgufValue, RawMetadata};
use gglib_core::domain::{ImageFamily, TensorTable, WeightsFormat};
use gglib_core::{GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, Quantization};

use crate::capabilities;
use crate::error::GgufResult;
use crate::format::{CONTEXT_LENGTH_KEYS, quantization};
use crate::reader::GgufReader;
use crate::tensor_table;

/// The fewest bytes a metadata pair takes in a file: the length of an empty
/// key, the value type, and a one-byte value.
pub(crate) const MIN_PAIR_BYTES: u64 = 8 + 4 + 1;

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

        // Read tensor count. It is held to the size of the file only when
        // the tensor-info table after the metadata is read.
        let tensor_count = if version >= 2 {
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
        let metadata_count =
            reader.declared_size("metadata count", metadata_count, MIN_PAIR_BYTES)?;

        // Parse metadata key-value pairs
        let mut raw_metadata = HashMap::new();
        for _ in 0..metadata_count {
            let key = reader.read_string()?;
            let value_type = reader.read_u32()?;
            let value = reader.read_value(value_type)?;
            raw_metadata.insert(key, value);
        }

        // Read on into the tensor-info table, the only place an image
        // model's GGUF says what it is. A table that cannot be read leaves
        // the family unknown and never fails a parse of the metadata.
        let image_family = match tensor_table::read_tensor_infos(&mut reader, tensor_count) {
            Ok(tensors) => ImageFamily::sniff(&TensorTable {
                format: WeightsFormat::Gguf,
                architecture: raw_metadata
                    .get("general.architecture")
                    .and_then(GgufValue::as_str)
                    .map(str::to_owned),
                tensors,
            }),
            Err(error) => {
                tracing::debug!(
                    path = %file_path.display(),
                    %error,
                    "the tensor table could not be read; no image family"
                );
                None
            }
        };

        // Extract structured metadata
        let mut metadata = extract_metadata(&raw_metadata, file_path);
        metadata.image_family = image_family;
        Ok(metadata)
    }
}

impl GgufParserPort for GgufParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        self.parse_internal(file_path).map_err(Into::into)
    }

    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities {
        capabilities::detect_all(&metadata.metadata)
    }

    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        tensor_table::read_file(path).map_err(Into::into)
    }

    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        tensor_table::read_head(head).map_err(Into::into)
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
    let architecture = extract_architecture(raw);
    let context_length = extract_context_length(raw, architecture.as_ref());
    let param_count_b = extract_param_count(raw, file_path);
    let quantization = extract_quantization(raw, file_path);

    // Extract MoE metadata
    let (expert_count, expert_used_count, expert_shared_count) =
        extract_moe_metadata(raw, architecture.as_ref());

    GgufMetadata {
        architecture,
        param_count_b,
        quantization,
        context_length,
        expert_count,
        expert_used_count,
        expert_shared_count,
        metadata: processed,
        role: crate::role::file_role(raw),
        image_family: None,
    }
}

/// Extract model architecture from metadata.
fn extract_architecture(raw: &RawMetadata) -> Option<String> {
    raw.get("general.architecture")
        .map(std::string::ToString::to_string)
}

/// Extract `MoE` (Mixture-of-Experts) metadata from architecture-specific keys.
fn extract_moe_metadata(
    raw: &RawMetadata,
    architecture: Option<&String>,
) -> (Option<u32>, Option<u32>, Option<u32>) {
    let Some(arch) = architecture else {
        return (None, None, None);
    };

    let expert_count = raw
        .get(&format!("{arch}.expert_count"))
        .and_then(GgufValue::as_u64)
        .and_then(|v| u32::try_from(v).ok());

    let expert_used_count = raw
        .get(&format!("{arch}.expert_used_count"))
        .and_then(GgufValue::as_u64)
        .and_then(|v| u32::try_from(v).ok());

    let expert_shared_count = raw
        .get(&format!("{arch}.expert_shared_count"))
        .and_then(GgufValue::as_u64)
        .and_then(|v| u32::try_from(v).ok());

    (expert_count, expert_used_count, expert_shared_count)
}

/// Extract context length from architecture-specific metadata.
fn extract_context_length(raw: &RawMetadata, architecture: Option<&String>) -> Option<u64> {
    // Try architecture-specific key first (e.g., "llama.context_length")
    if let Some(arch) = architecture {
        let arch_key = format!("{arch}.context_length");
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
/// For `MoE` models ("NxM.MB" format), returns TOTAL parameters (N × M) for VRAM estimation.
fn parse_param_label(size_label: &str) -> Option<f64> {
    let upper = size_label.to_uppercase();

    // Handle MoE format: "8x7B", "64x2.6B", "512x2.5B" (NxM.MB)
    // Calculate TOTAL parameters: N × M (e.g., 64 × 2.6 = 166.4B)
    if let Some(x_pos) = upper.find('X') {
        let before_x = &upper[..x_pos];
        let after_x = &upper[x_pos + 1..];

        if let Some(expert_size_str) = after_x.strip_suffix('B') {
            if let (Ok(expert_count), Ok(expert_size)) =
                (before_x.parse::<f64>(), expert_size_str.parse::<f64>())
            {
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
    let quant = Quantization::from_filename(filename);
    if quant.is_unknown() {
        "Unknown".to_string()
    } else {
        quant.as_str().to_string()
    }
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
#[path = "parser_tests.rs"]
mod tests;
