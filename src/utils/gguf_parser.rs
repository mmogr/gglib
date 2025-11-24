#![allow(clippy::collapsible_if)]

//! GGUF file parser for extracting metadata from GGUF model files.
//!
//! This module provides functionality to parse GGUF file headers and extract
//! rich metadata including model architecture, quantization, context length,
//! and other model-specific information.

use crate::models::GgufMetadata;
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// GGUF magic number (4 bytes): "GGUF"
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46]; // "GGUF"

/// GGUF metadata value types
#[derive(Debug, Clone)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    String(String),
    Array(Vec<GgufValue>),
    U64(u64),
    I64(i64),
    F64(f64),
}

impl fmt::Display for GgufValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GgufValue::U8(v) => write!(f, "{v}"),
            GgufValue::I8(v) => write!(f, "{v}"),
            GgufValue::U16(v) => write!(f, "{v}"),
            GgufValue::I16(v) => write!(f, "{v}"),
            GgufValue::U32(v) => write!(f, "{v}"),
            GgufValue::I32(v) => write!(f, "{v}"),
            GgufValue::F32(v) => write!(f, "{v}"),
            GgufValue::Bool(v) => write!(f, "{v}"),
            GgufValue::String(v) => write!(f, "{v}"),
            GgufValue::U64(v) => write!(f, "{v}"),
            GgufValue::I64(v) => write!(f, "{v}"),
            GgufValue::F64(v) => write!(f, "{v}"),
            GgufValue::Array(arr) => {
                // Limit array output to prevent massive tokenizer vocab dumps
                if arr.len() > 10 {
                    write!(f, "[Array with {} elements]", arr.len())
                } else {
                    write!(
                        f,
                        "[{}]",
                        arr.iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
        }
    }
}

impl GgufValue {
    /// Try to convert the value to a u64
    ///
    /// Attempts to convert various numeric GGUF value types to u64.
    /// Only converts non-negative values to avoid overflow issues.
    ///
    /// # Returns
    /// * `Some(u64)` if the value can be safely converted
    /// * `None` if the value is negative, non-numeric, or would overflow
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gglib::utils::gguf_parser::GgufValue;
    ///
    /// let value_u32 = GgufValue::U32(4096);
    /// assert_eq!(value_u32.as_u64(), Some(4096));
    ///
    /// let value_i32_negative = GgufValue::I32(-1);
    /// assert_eq!(value_i32_negative.as_u64(), None);
    ///
    /// let value_string = GgufValue::String("hello".to_string());
    /// assert_eq!(value_string.as_u64(), None);
    /// ```
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            GgufValue::U8(v) => Some(*v as u64),
            GgufValue::U16(v) => Some(*v as u64),
            GgufValue::U32(v) => Some(*v as u64),
            GgufValue::U64(v) => Some(*v),
            GgufValue::I8(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I16(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I32(v) if *v >= 0 => Some(*v as u64),
            GgufValue::I64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    /// Try to convert the value to a f64
    ///
    /// Attempts to convert various numeric GGUF value types to f64.
    /// This is useful for extracting floating-point metadata values
    /// regardless of their original storage type.
    ///
    /// # Returns
    /// * `Some(f64)` if the value can be converted to a float
    /// * `None` if the value is non-numeric (String, Array, Bool)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gglib::utils::gguf_parser::GgufValue;
    ///
    /// let value_f32 = GgufValue::F32(7.5);
    /// assert_eq!(value_f32.as_f64(), Some(7.5));
    ///
    /// let value_u64 = GgufValue::U64(1000);
    /// assert_eq!(value_u64.as_f64(), Some(1000.0));
    ///
    /// let value_bool = GgufValue::Bool(true);
    /// assert_eq!(value_bool.as_f64(), None);
    /// ```
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            GgufValue::F32(v) => Some(*v as f64),
            GgufValue::F64(v) => Some(*v),
            GgufValue::U8(v) => Some(*v as f64),
            GgufValue::U16(v) => Some(*v as f64),
            GgufValue::U32(v) => Some(*v as f64),
            GgufValue::U64(v) => Some(*v as f64),
            GgufValue::I8(v) => Some(*v as f64),
            GgufValue::I16(v) => Some(*v as f64),
            GgufValue::I32(v) => Some(*v as f64),
            GgufValue::I64(v) => Some(*v as f64),
            _ => None,
        }
    }
}

/// Parse GGUF metadata from a file
///
/// Reads and parses the header of a GGUF file to extract model metadata
/// including architecture, quantization, parameter count, and context length.
///
/// # Arguments
/// * `file_path` - Path to the GGUF file to parse
///
/// # Returns
/// * `Ok(GgufMetadata)` with extracted model information
/// * `Err` if file cannot be read or is not a valid GGUF file
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::utils::gguf_parser::parse_gguf_metadata;
/// use std::path::Path;
///
/// // Parse metadata from a GGUF file
/// let metadata = parse_gguf_metadata("./models/llama-2-7b-chat.gguf")?;
///
/// if let Some(name) = &metadata.name {
///     println!("Model: {}", name);
/// }
/// if let Some(arch) = &metadata.architecture {
///     println!("Architecture: {}", arch);
/// }
/// if let Some(params) = metadata.param_count_b {
///     println!("Parameters: {:.1}B", params);
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse_gguf_metadata<P: AsRef<Path>>(file_path: P) -> Result<GgufMetadata> {
    let path = file_path.as_ref();
    let file =
        File::open(path).with_context(|| format!("Failed to open file: {}", path.display()))?;

    let mut reader = BufReader::new(file);

    // Read and validate magic number
    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .context("Failed to read GGUF magic number")?;

    if magic != GGUF_MAGIC {
        return Err(anyhow!("Invalid GGUF file: wrong magic number"));
    }

    // Read version
    let version = read_u32(&mut reader)?;
    if !(1..=3).contains(&version) {
        return Err(anyhow!("Unsupported GGUF version: {}", version));
    }

    // Read tensor count (but we don't need to store it)
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
    let mut metadata_map = HashMap::new();

    for _ in 0..metadata_count {
        let key = read_string(&mut reader)?;
        let value_type = read_u32(&mut reader)?;
        let value = read_value(&mut reader, value_type, version)?;

        metadata_map.insert(key, value);
    }

    // Extract specific metadata fields
    let metadata = extract_metadata_fields(&metadata_map, path)?;

    Ok(GgufMetadata {
        name: metadata.name,
        architecture: metadata.architecture,
        param_count_b: metadata.param_count_b,
        quantization: metadata.quantization,
        context_length: metadata.context_length,
        metadata: metadata.metadata,
    })
}

/// Extract and process specific metadata fields
fn extract_metadata_fields(
    metadata_map: &HashMap<String, GgufValue>,
    file_path: &Path,
) -> Result<GgufMetadata> {
    let mut processed_metadata = HashMap::new();

    // Convert metadata to string representation for storage, but skip large arrays
    for (key, value) in metadata_map {
        // Skip tokenizer arrays and other large data that would clutter the metadata
        if key.starts_with("tokenizer.")
            && matches!(value, GgufValue::Array(arr) if arr.len() > 100)
        {
            // Store just a summary for large tokenizer arrays
            if let GgufValue::Array(arr) = value {
                processed_metadata
                    .insert(key.clone(), format!("Array with {} elements", arr.len()));
            }
        } else if key == "tokenizer.chat_template" {
            // Truncate very long chat templates
            let template_str = value.to_string();
            if template_str.len() > 500 {
                processed_metadata.insert(
                    key.clone(),
                    format!("Chat template ({} chars)", template_str.len()),
                );
            } else {
                processed_metadata.insert(key.clone(), template_str);
            }
        } else {
            processed_metadata.insert(key.clone(), value.to_string());
        }
    }

    // Extract model name
    let name = metadata_map
        .get("general.name")
        .map(|v| v.to_string())
        .or_else(|| {
            // Fallback to filename without extension
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        });

    // Extract architecture
    let architecture = metadata_map
        .get("general.architecture")
        .map(|v| v.to_string());

    // Debug: Print available metadata keys that might contain context length
    if std::env::var("GGLIB_DEBUG").is_ok() {
        println!("Available metadata keys:");
        for key in metadata_map.keys() {
            if key.contains("context") || key.contains("length") || key.contains("size") {
                println!("  {}: {}", key, metadata_map[key]);
            }
        }
    }

    // Extract context length (try multiple possible keys)
    let context_length = metadata_map
        .get("llama.context_length")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            metadata_map
                .get("gpt2.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("bert.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("mistral.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("qwen.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("qwen2.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("qwen3.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| {
            metadata_map
                .get("qwen3moe.context_length")
                .and_then(|v| v.as_u64())
        })
        .or_else(|| metadata_map.get("context_length").and_then(|v| v.as_u64()));

    // Try to extract parameter count from various sources
    let param_count_b = extract_parameter_count(metadata_map, file_path)?;

    // Extract quantization info
    let quantization = extract_quantization_info(metadata_map, file_path);

    Ok(GgufMetadata {
        name,
        architecture,
        param_count_b,
        quantization,
        context_length,
        metadata: processed_metadata,
    })
}

/// Extract parameter count from metadata or estimate from filename
fn extract_parameter_count(
    metadata_map: &HashMap<String, GgufValue>,
    file_path: &Path,
) -> Result<Option<f64>> {
    // Try to get from metadata first
    if let Some(size_label) = metadata_map.get("general.size_label") {
        if let Some(params) = parse_parameter_count_from_size_label(&size_label.to_string()) {
            return Ok(Some(params));
        }
    }

    // Fallback to parsing filename
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        if let Some(params) = parse_parameter_count_from_filename(filename) {
            return Ok(Some(params));
        }
    }

    Ok(None)
}

/// Parse parameter count from size label (e.g., "7B", "13B", "70B")
fn parse_parameter_count_from_size_label(size_label: &str) -> Option<f64> {
    let size_upper = size_label.to_uppercase();

    // Handle formats like "7B", "13B", "70B"
    if size_upper.ends_with('B') {
        let number_part = &size_upper[..size_upper.len() - 1];
        if let Ok(num) = number_part.parse::<f64>() {
            return Some(num);
        }
    }

    // Handle formats like "8x7B" (mixture of experts)
    if let Some(x_pos) = size_upper.find('X') {
        let after_x = &size_upper[x_pos + 1..];
        if let Some(number_part) = after_x.strip_suffix('B') {
            if let Ok(num) = number_part.parse::<f64>() {
                return Some(num);
            }
        }
    }

    None
}

/// Parse parameter count from filename
fn parse_parameter_count_from_filename(filename: &str) -> Option<f64> {
    let filename_upper = filename.to_uppercase();

    // Look for patterns like "7B", "13B", etc.
    let patterns = ["B", "BILLION"];

    for pattern in &patterns {
        if let Some(pos) = filename_upper.find(pattern) {
            // Look backwards for the number
            let before_pattern = &filename_upper[..pos];

            // Find the last sequence of digits (possibly with decimal point)
            let mut number_str = String::new();
            let mut found_digit = false;

            for ch in before_pattern.chars().rev() {
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

/// Extract quantization information from metadata or filename
fn extract_quantization_info(
    metadata_map: &HashMap<String, GgufValue>,
    file_path: &Path,
) -> Option<String> {
    // First try filename parsing as it's often more specific and accurate
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        if let Some(filename_quant) = extract_quantization_from_filename(filename) {
            return Some(filename_quant);
        }
    }

    // Fallback to metadata
    if let Some(file_type) = metadata_map.get("general.file_type") {
        let file_type_str = file_type.to_string();
        if let Some(quant) = map_file_type_to_quantization(&file_type_str) {
            return Some(quant);
        }
    }

    None
}

/// Map GGUF file type to quantization string
fn map_file_type_to_quantization(file_type: &str) -> Option<String> {
    // This is a simplified mapping - in reality, GGUF file types are more complex
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

/// Extract quantization info from filename
fn extract_quantization_from_filename(filename: &str) -> Option<String> {
    let filename_upper = filename.to_uppercase();

    // Common quantization patterns (ordered by specificity - longer patterns first)
    let quantizations = [
        "Q8_K_XL", "Q8_K_XXL", "Q8_K_L", "Q8_K_M", "Q8_K_S", "Q8_K", "Q6_K_XL", "Q6_K_XXL",
        "Q6_K_L", "Q6_K_M", "Q6_K_S", "Q6_K", "Q5_K_XL", "Q5_K_XXL", "Q5_K_L", "Q5_K_M", "Q5_K_S",
        "Q5_K", "Q4_K_XL", "Q4_K_XXL", "Q4_K_L", "Q4_K_M", "Q4_K_S", "Q4_K", "Q3_K_XL", "Q3_K_XXL",
        "Q3_K_L", "Q3_K_M", "Q3_K_S", "Q3_K", "Q2_K_XL", "Q2_K_XXL", "Q2_K_L", "Q2_K_M", "Q2_K_S",
        "Q2_K", "Q8_0", "Q5_0", "Q5_1", "Q4_0", "Q4_1", "F16", "F32",
    ];

    for quant in &quantizations {
        if filename_upper.contains(quant) {
            return Some(quant.to_string());
        }
    }

    None
}

/// Read a u32 value from the reader
fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Read a u64 value from the reader
fn read_u64<R: Read>(reader: &mut R) -> Result<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Read a string from the reader
fn read_string<R: Read>(reader: &mut R) -> Result<String> {
    let len = read_u64(reader)? as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).context("Invalid UTF-8 string in GGUF file")
}

/// Read a value based on its type
fn read_value<R: Read>(reader: &mut R, value_type: u32, _version: u32) -> Result<GgufValue> {
    match value_type {
        0 => Ok(GgufValue::U8(read_u8(reader)?)),
        1 => Ok(GgufValue::I8(read_i8(reader)?)),
        2 => Ok(GgufValue::U16(read_u16(reader)?)),
        3 => Ok(GgufValue::I16(read_i16(reader)?)),
        4 => Ok(GgufValue::U32(read_u32(reader)?)),
        5 => Ok(GgufValue::I32(read_i32(reader)?)),
        6 => Ok(GgufValue::F32(read_f32(reader)?)),
        7 => Ok(GgufValue::Bool(read_bool(reader)?)),
        8 => Ok(GgufValue::String(read_string(reader)?)),
        9 => {
            // Array type
            let element_type = read_u32(reader)?;
            let count = read_u64(reader)? as usize;
            let mut elements = Vec::with_capacity(count);

            for _ in 0..count {
                elements.push(read_value(reader, element_type, _version)?);
            }

            Ok(GgufValue::Array(elements))
        }
        10 => Ok(GgufValue::U64(read_u64(reader)?)),
        11 => Ok(GgufValue::I64(read_i64(reader)?)),
        12 => Ok(GgufValue::F64(read_f64(reader)?)),
        _ => Err(anyhow!("Unknown GGUF value type: {}", value_type)),
    }
}

/// Helper functions for reading basic types
fn read_u8<R: Read>(reader: &mut R) -> Result<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_i8<R: Read>(reader: &mut R) -> Result<i8> {
    Ok(read_u8(reader)? as i8)
}

fn read_u16<R: Read>(reader: &mut R) -> Result<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_i16<R: Read>(reader: &mut R) -> Result<i16> {
    Ok(read_u16(reader)? as i16)
}

fn read_i32<R: Read>(reader: &mut R) -> Result<i32> {
    Ok(read_u32(reader)? as i32)
}

fn read_f32<R: Read>(reader: &mut R) -> Result<f32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

fn read_bool<R: Read>(reader: &mut R) -> Result<bool> {
    Ok(read_u8(reader)? != 0)
}

fn read_i64<R: Read>(reader: &mut R) -> Result<i64> {
    Ok(read_u64(reader)? as i64)
}

fn read_f64<R: Read>(reader: &mut R) -> Result<f64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}
