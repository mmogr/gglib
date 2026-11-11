//! GGUF parser implementation.
//!
//! This module provides the main `GgufParser` struct that implements
//! the `GgufParserPort` trait from `gglib-core`.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use gglib_core::domain::gguf::{GgufValue, RawMetadata};
use gglib_core::domain::{FileType, ImageFamily, TensorTable, WeightsFormat};
use gglib_core::{
    GgufCapabilities, GgufFileRole, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata,
};

use crate::capabilities;
use crate::error::GgufResult;
use crate::format::CONTEXT_LENGTH_KEYS;
use crate::reader::GgufReader;
use crate::{head, tensor_table};

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
}

/// The metadata of the GGUF file `reader` reads from its start, which
/// `file_path` names: the name is read for the parameter count when the
/// header has none, and never for the type.
pub(crate) fn parse_internal<R: Read>(
    mut reader: GgufReader<R>,
    file_path: &Path,
) -> GgufResult<GgufMetadata> {
    let file_len = reader.remaining();
    reader.read_magic()?;
    // The tensor count is held to the size of the file only when the
    // tensor-info table after the metadata is read.
    let (tensor_count, metadata_count) = reader.read_counts()?;
    let metadata_count = reader.declared_size("metadata count", metadata_count, MIN_PAIR_BYTES)?;

    let mut raw_metadata = HashMap::new();
    for _ in 0..metadata_count {
        let key = reader.read_string()?;
        let value_type = reader.read_u32()?;
        let value = reader.read_value(value_type)?;
        raw_metadata.insert(key, value);
    }

    let mut metadata = extract_metadata(&raw_metadata, file_path);
    metadata.role = crate::role::file_role(&raw_metadata, tensor_count);

    // Read on into the tensor-info table, the only place an image model's
    // GGUF says what it is, and the end of which is where the tensor data
    // starts. A table that cannot be read leaves the family and the payload
    // unknown and never fails a parse of the metadata.
    match tensor_table::read_tensor_infos(&mut reader, tensor_count) {
        Ok(tensors) => {
            let table_end = file_len - reader.remaining();
            metadata.payload = tensor_table::payload(&raw_metadata, &tensors, file_len, table_end);
            metadata.image_family = ImageFamily::sniff(&TensorTable {
                format: WeightsFormat::Gguf,
                architecture: metadata.architecture.clone(),
                tensors,
            });
        }
        Err(error) => {
            tracing::debug!(
                path = %file_path.display(),
                %error,
                "the tensor table could not be read; no image family"
            );
        }
    }
    Ok(metadata)
}

impl GgufParserPort for GgufParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        GgufReader::open(file_path)
            .and_then(|reader| parse_internal(reader, file_path))
            .map_err(Into::into)
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

    fn parse_head(&self, head: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        head::read(head, file_name).map_err(Into::into)
    }
}

// =============================================================================
// Metadata Extraction
// =============================================================================

/// Extract structured metadata from raw GGUF key-value pairs.
pub(crate) fn extract_metadata(raw: &RawMetadata, file_path: &Path) -> GgufMetadata {
    let processed = raw
        .iter()
        .map(|(key, value)| (key.clone(), stored_text(key, value)))
        .collect();

    // Extract fields
    let architecture = extract_architecture(raw);
    let context_length = extract_context_length(raw, architecture.as_ref());
    let param_count_b = extract_param_count(raw, file_path);
    let file_type = extract_file_type(raw);

    // Extract MoE metadata
    let (expert_count, expert_used_count, expert_shared_count) =
        extract_moe_metadata(raw, architecture.as_ref());

    GgufMetadata {
        architecture,
        param_count_b,
        file_type,
        payload: None,
        context_length,
        expert_count,
        expert_used_count,
        expert_shared_count,
        metadata: processed,
        role: GgufFileRole::Weights,
        image_family: None,
    }
}

/// The text a value is stored as. A tokenizer's large arrays are summarised.
/// A number or bool array is kept whole, as `[16, 16, 4]` (JSON for ints and bools),
/// so a per-layer list such as `{arch}.attention.head_count_kv` can be read
/// back. Any other value is its `Display`, which summarises a long array.
fn stored_text(key: &str, value: &GgufValue) -> String {
    match value {
        GgufValue::Array(items) if key.starts_with("tokenizer.") && items.len() > 100 => {
            format!("Array with {} elements", items.len())
        }
        GgufValue::Array(items)
            if items
                .iter()
                .all(|item| !matches!(item, GgufValue::String(_) | GgufValue::Array(_))) =>
        {
            let items: Vec<String> = items.iter().map(ToString::to_string).collect();
            format!("[{}]", items.join(", "))
        }
        _ => value.to_string(),
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

/// The parameter count a size label states: `7B`, `13B`, `70B`.
///
/// A mixture of experts' label, `NxMB` (`8x7B`, `512x56B`), states none.
/// llama.cpp's converter writes M as the shared weights and one expert's
/// together, so the total, shared plus N experts, is not in the label, and
/// N × M can be off by any factor: `512x56B` is 177B, not 28672B. Its count
/// is the tensors' (`HeaderFacts::param_count_b`), or unknown.
fn parse_param_label(size_label: &str) -> Option<f64> {
    let upper = size_label.to_uppercase();

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
                // `8x7B` is a mixture of experts' label, which states no
                // total (see `parse_param_label`).
                let rest = before
                    .rfind(number_str.as_str())
                    .map_or("", |at| &before[..at]);
                let of_experts = rest
                    .strip_suffix('X')
                    .is_some_and(|count| count.ends_with(|c: char| c.is_ascii_digit()));
                return (!of_experts).then_some(num);
            }
        }
    }

    None
}

/// The type `general.file_type` states, which llama.cpp's converter and
/// quantizer write as an unsigned integer.
fn extract_file_type(raw: &RawMetadata) -> Option<FileType> {
    raw.get("general.file_type")
        .and_then(GgufValue::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .map(FileType)
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
