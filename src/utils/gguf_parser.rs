#![allow(clippy::collapsible_if)]

//! GGUF file parser for extracting metadata from GGUF model files.
//!
//! This module provides functionality to parse GGUF file headers and extract
//! rich metadata including model architecture, quantization, context length,
//! and other model-specific information.

use crate::commands::download::extract_quantization_from_filename;
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
            // Preserve full chat template for tool/reasoning detection
            // Full templates are needed for accurate capability detection
            processed_metadata.insert(key.clone(), value.to_string());
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
    // First try filename parsing using the canonical implementation (DRY)
    if let Some(filename) = file_path.file_name().and_then(|s| s.to_str()) {
        let quant = extract_quantization_from_filename(filename);
        if quant != "unknown" {
            return Some(quant.to_string());
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

/// Known thinking/reasoning tag patterns used by various models.
/// These are checked against the chat template to detect reasoning model support.
const THINKING_TAG_PATTERNS: &[&str] = &[
    // Standard patterns (DeepSeek R1, Qwen3, most reasoning models)
    "<think>",
    "<think ",
    "</think>",
    // Alternative tag names
    "<reasoning>",
    "</reasoning>",
    // Seed-OSS models
    "<seed:think>",
    "</seed:think>",
    // Command-R7B style
    "<|START_THINKING|>",
    "<|END_THINKING|>",
    // Apertus style
    "<|inner_prefix|>",
    "<|inner_suffix|>",
    // Nemotron V2 style (uses same <think> but in different context)
    "enable_thinking",
    // Bailing/Ring models
    "thinking_forced_open",
];

/// Known model name patterns that indicate reasoning capability.
/// Checked against the model name for additional detection confidence.
const REASONING_MODEL_NAME_PATTERNS: &[&str] = &[
    "deepseek-r1",
    "deepseek-v3",
    "qwen3",
    "qwq",
    "thinking",
    "reasoning",
    "cot", // chain-of-thought
    "o1",
    "o3",
];

// =============================================================================
// Tool Calling / Function Calling Detection
// =============================================================================

/// Known tool calling patterns found in chat templates.
/// These patterns indicate the model supports function/tool calling.
/// Based on llama.cpp's tool call handler detection (PR #9639).
///
/// Note: This constant documents the patterns we detect. The actual detection
/// uses inline constants with scores in `detect_tool_support()` for finer control.
#[allow(dead_code)]
const TOOL_CALLING_PATTERNS: &[&str] = &[
    // Hermes/NousResearch style (most common)
    "<tool_call>",
    "</tool_call>",
    "<tool_response>",
    // Llama 3.x native tool calling
    "<|python_tag|>",
    "ipython",
    // Functionary style
    ">>>",
    "from functions import",
    // Mistral/MistralAI style
    "[TOOL_CALLS]",
    "[TOOL_RESULTS]",
    // Firefunction style
    "functools[",
    // DeepSeek function calling
    "<｜tool▁calls▁begin｜>",
    "<｜tool▁call▁begin｜>",
    // Generic tool/function indicators in templates
    "tools",
    "tool_call",
    "function_call",
    "available_tools",
    // Jinja template conditionals for tools
    "if tools",
    "tools is defined",
    "tools | length",
];

/// Model name patterns that indicate tool calling capability.
/// More conservative than template detection.
const TOOL_CALLING_MODEL_NAME_PATTERNS: &[&str] = &[
    "hermes",       // NousResearch Hermes models
    "functionary",  // MeetKai Functionary models
    "firefunction", // Fireworks Firefunction
    "toolcall",
    "function",
    "agent",
];

/// Result of tool calling capability detection
#[derive(Debug, Clone)]
pub struct ToolCallingDetection {
    /// Whether the model appears to support tool/function calling
    pub supports_tool_calling: bool,
    /// Confidence level of the detection (0.0 to 1.0)
    pub confidence: f32,
    /// The specific pattern(s) that matched, if any
    pub matched_patterns: Vec<String>,
    /// Detected tool calling format (e.g., "hermes", "llama3", "mistral")
    pub detected_format: Option<String>,
}

impl Default for ToolCallingDetection {
    fn default() -> Self {
        Self {
            supports_tool_calling: false,
            confidence: 0.0,
            matched_patterns: Vec::new(),
            detected_format: None,
        }
    }
}

/// Result of reasoning capability detection
#[derive(Debug, Clone)]
pub struct ReasoningDetection {
    /// Whether the model appears to support reasoning/thinking
    pub supports_reasoning: bool,
    /// Confidence level of the detection (0.0 to 1.0)
    pub confidence: f32,
    /// The specific pattern(s) that matched, if any
    pub matched_patterns: Vec<String>,
    /// Suggested reasoning format for llama-server
    pub suggested_format: Option<String>,
}

impl Default for ReasoningDetection {
    fn default() -> Self {
        Self {
            supports_reasoning: false,
            confidence: 0.0,
            matched_patterns: Vec::new(),
            suggested_format: None,
        }
    }
}

/// Detect if a model supports reasoning/thinking based on its GGUF metadata.
///
/// This function analyzes the chat template and model name to determine
/// if the model is a reasoning model that outputs `<think>` or similar tags.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// A `ReasoningDetection` struct with detection results and confidence
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use gglib::utils::gguf_parser::detect_reasoning_support;
///
/// let mut metadata = HashMap::new();
/// metadata.insert(
///     "tokenizer.chat_template".to_string(),
///     "... <think> ... </think> ...".to_string()
/// );
///
/// let detection = detect_reasoning_support(&metadata);
/// assert!(detection.supports_reasoning);
/// assert!(detection.confidence > 0.5);
/// ```
pub fn detect_reasoning_support(metadata: &HashMap<String, String>) -> ReasoningDetection {
    let mut detection = ReasoningDetection::default();
    let mut confidence_score = 0.0f32;

    // Check chat template for thinking patterns (highest confidence)
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        for pattern in THINKING_TAG_PATTERNS {
            let pattern_lower = pattern.to_lowercase();
            if template_lower.contains(&pattern_lower) {
                detection.matched_patterns.push(pattern.to_string());
                // Opening tags are higher confidence than closing tags alone
                if pattern.starts_with('<') && !pattern.starts_with("</") {
                    confidence_score += 0.4;
                } else {
                    confidence_score += 0.2;
                }
            }
        }

        // Check for template variables that indicate thinking support
        if template_lower.contains("enable_thinking")
            || template_lower.contains("thinking_forced_open")
        {
            confidence_score += 0.3;
        }
    }

    // Check model name for reasoning patterns
    // High-confidence patterns (definitive reasoning models) get 0.4
    // Medium-confidence patterns (might be reasoning) get 0.25
    if let Some(name) = metadata.get("general.name") {
        let name_lower = name.to_lowercase();

        // High-confidence: definitive reasoning model names
        const HIGH_CONFIDENCE_PATTERNS: &[&str] = &[
            "deepseek-r1", // DeepSeek R1 family
            "qwq",         // Qwen QwQ reasoning model
            "o1",          // OpenAI O1 style
            "o3",          // OpenAI O3 style
        ];

        for pattern in HIGH_CONFIDENCE_PATTERNS {
            if name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{}", pattern));
                confidence_score += 0.4;
            }
        }

        // Medium-confidence: might indicate reasoning
        for pattern in REASONING_MODEL_NAME_PATTERNS {
            // Skip patterns already checked with high confidence
            if HIGH_CONFIDENCE_PATTERNS.contains(pattern) {
                continue;
            }
            if name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{}", pattern));
                confidence_score += 0.25;
            }
        }
    }

    // Check architecture for known reasoning architectures
    if let Some(arch) = metadata.get("general.architecture") {
        let arch_lower = arch.to_lowercase();
        // DeepSeek models often have specific architecture markers
        if arch_lower.contains("deepseek") {
            confidence_score += 0.15;
            detection
                .matched_patterns
                .push(format!("arch:{}", arch_lower));
        }
    }

    // Normalize confidence to 0.0-1.0 range
    detection.confidence = confidence_score.min(1.0);
    detection.supports_reasoning = detection.confidence >= 0.3;

    // Suggest format based on detected patterns
    if detection.supports_reasoning {
        // Most reasoning models work with "deepseek" format
        detection.suggested_format = Some("deepseek".to_string());
    }

    detection
}

/// Check if a model's metadata indicates it's a reasoning model.
/// This is a simplified boolean check for common use cases.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// `true` if the model appears to be a reasoning model, `false` otherwise
pub fn is_reasoning_model(metadata: &HashMap<String, String>) -> bool {
    detect_reasoning_support(metadata).supports_reasoning
}

/// Apply reasoning detection to GGUF metadata and return tags to add.
///
/// This is a shared helper function used by all model add flows:
/// - CLI `add` command
/// - GUI "Add Model" from local file
/// - HuggingFace browser downloads
///
/// It analyzes the metadata, logs the detection results to stdout,
/// and returns a list of tags to apply to the model.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// A `Vec<String>` of tags to add to the model (e.g., `["reasoning"]`)
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use gglib::utils::gguf_parser::apply_reasoning_detection;
///
/// let mut metadata = HashMap::new();
/// metadata.insert(
///     "tokenizer.chat_template".to_string(),
///     "... <think> ... </think> ...".to_string()
/// );
///
/// let tags = apply_reasoning_detection(&metadata);
/// assert!(tags.contains(&"reasoning".to_string()));
/// ```
pub fn apply_reasoning_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let detection = detect_reasoning_support(metadata);
    let mut tags = Vec::new();

    if detection.supports_reasoning {
        println!("\n🧠 Detected reasoning model capabilities:");
        println!("  Confidence: {:.0}%", detection.confidence * 100.0);
        if !detection.matched_patterns.is_empty() {
            println!(
                "  Matched patterns: {}",
                detection.matched_patterns.join(", ")
            );
        }
        if let Some(ref format) = detection.suggested_format {
            println!("  Suggested format: --reasoning-format {}", format);
        }
        println!("  → Auto-adding 'reasoning' tag for optimal llama-server configuration");
        tags.push("reasoning".to_string());
    }

    tags
}

/// Get the full chat template from metadata, even if it was truncated.
/// Returns None if the template was truncated (stored as "Chat template (N chars)").
///
/// This is useful when you need to re-parse the GGUF file to get the full template.
pub fn get_chat_template(metadata: &HashMap<String, String>) -> Option<&String> {
    metadata.get("tokenizer.chat_template").and_then(|s| {
        // Check if it was truncated
        if s.starts_with("Chat template (") && s.ends_with(" chars)") {
            None // Was truncated, need to re-read from file
        } else {
            Some(s)
        }
    })
}

// =============================================================================
// Tool Calling Detection Functions
// =============================================================================

/// Detect if a model supports tool/function calling based on its GGUF metadata.
///
/// This function analyzes the chat template and model name to determine
/// if the model supports tool calling (function calling) via the OpenAI-compatible API.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// A `ToolCallingDetection` struct with detection results and confidence
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use gglib::utils::gguf_parser::detect_tool_support;
///
/// let mut metadata = HashMap::new();
/// metadata.insert(
///     "tokenizer.chat_template".to_string(),
///     "{% if tools %}<tool_call>{{ tool }}</tool_call>{% endif %}".to_string()
/// );
///
/// let detection = detect_tool_support(&metadata);
/// assert!(detection.supports_tool_calling);
/// ```
pub fn detect_tool_support(metadata: &HashMap<String, String>) -> ToolCallingDetection {
    let mut detection = ToolCallingDetection::default();
    let mut confidence_score = 0.0f32;

    // Track which format we detected for format-specific handling
    let mut detected_formats: Vec<&str> = Vec::new();

    // Check chat template for tool calling patterns (highest confidence)
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        // High-confidence patterns (explicit tool calling syntax)
        const HIGH_CONFIDENCE_PATTERNS: &[(&str, &str, f32)] = &[
            ("<tool_call>", "hermes", 0.5),
            ("</tool_call>", "hermes", 0.3),
            ("<tool_response>", "hermes", 0.3),
            ("[tool_calls]", "mistral", 0.5),
            ("[tool_results]", "mistral", 0.3),
            ("<｜tool▁calls▁begin｜>", "deepseek", 0.5),
            ("<｜tool▁call▁begin｜>", "deepseek", 0.4),
            ("<|python_tag|>", "llama3", 0.5),
            ("functools[", "firefunction", 0.5),
            (">>>", "functionary", 0.3),
            ("from functions import", "functionary", 0.4),
        ];

        for (pattern, format, score) in HIGH_CONFIDENCE_PATTERNS {
            if template_lower.contains(&pattern.to_lowercase()) {
                detection.matched_patterns.push(pattern.to_string());
                confidence_score += score;
                if !detected_formats.contains(format) {
                    detected_formats.push(format);
                }
            }
        }

        // Medium-confidence patterns (Jinja conditionals that handle tools)
        const MEDIUM_CONFIDENCE_PATTERNS: &[&str] = &[
            "if tools",
            "tools is defined",
            "tools | length",
            "available_tools",
        ];

        for pattern in MEDIUM_CONFIDENCE_PATTERNS {
            if template_lower.contains(&pattern.to_lowercase()) {
                detection
                    .matched_patterns
                    .push(format!("jinja:{}", pattern));
                confidence_score += 0.35;
            }
        }

        // Low-confidence patterns (might just be documentation)
        if template_lower.contains("tool_call")
            && !detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("tool_call"))
        {
            detection.matched_patterns.push("tool_call".to_string());
            confidence_score += 0.2;
        }
        if template_lower.contains("function_call") {
            detection.matched_patterns.push("function_call".to_string());
            confidence_score += 0.2;
        }
    }

    // Check model name for tool calling patterns
    if let Some(name) = metadata.get("general.name") {
        let name_lower = name.to_lowercase();

        for pattern in TOOL_CALLING_MODEL_NAME_PATTERNS {
            if name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{}", pattern));
                // Higher confidence for explicit tool-calling model names
                if *pattern == "hermes" || *pattern == "functionary" || *pattern == "firefunction" {
                    confidence_score += 0.4;
                    if *pattern == "hermes" && !detected_formats.contains(&"hermes") {
                        detected_formats.push("hermes");
                    }
                } else {
                    confidence_score += 0.25;
                }
            }
        }
    }

    // Normalize confidence to 0.0-1.0 range
    detection.confidence = confidence_score.min(1.0);
    detection.supports_tool_calling = detection.confidence >= 0.3;

    // Set detected format (prefer explicit template detection over name-based)
    if !detected_formats.is_empty() {
        detection.detected_format = Some(detected_formats[0].to_string());
    }

    detection
}

/// Check if a model's metadata indicates it supports tool calling.
/// This is a simplified boolean check for common use cases.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// `true` if the model appears to support tool calling, `false` otherwise
pub fn is_tool_capable_model(metadata: &HashMap<String, String>) -> bool {
    detect_tool_support(metadata).supports_tool_calling
}

/// Apply tool calling detection to GGUF metadata and return tags to add.
///
/// This is a shared helper function used by all model add flows:
/// - CLI `add` command
/// - GUI "Add Model" from local file
/// - HuggingFace browser downloads
///
/// It analyzes the metadata, logs the detection results to stdout,
/// and returns a list of tags to apply to the model.
///
/// Note: Returns "agent" tag (not "tools") because:
/// 1. "agent" already triggers --jinja auto-enable in resolve_jinja_flag()
/// 2. More semantically accurate - tool calling enables agentic capabilities
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// A `Vec<String>` of tags to add to the model (e.g., `["agent"]`)
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use gglib::utils::gguf_parser::apply_tool_detection;
///
/// let mut metadata = HashMap::new();
/// metadata.insert(
///     "tokenizer.chat_template".to_string(),
///     "{% if tools %}<tool_call>{{ tool }}</tool_call>{% endif %}".to_string()
/// );
///
/// let tags = apply_tool_detection(&metadata);
/// assert!(tags.contains(&"agent".to_string()));
/// ```
pub fn apply_tool_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let detection = detect_tool_support(metadata);
    let mut tags = Vec::new();

    if detection.supports_tool_calling {
        println!("\n🔧 Detected tool calling capabilities:");
        println!("  Confidence: {:.0}%", detection.confidence * 100.0);
        if !detection.matched_patterns.is_empty() {
            println!(
                "  Matched patterns: {}",
                detection.matched_patterns.join(", ")
            );
        }
        if let Some(ref format) = detection.detected_format {
            println!("  Detected format: {}", format);
        }
        println!("  → Auto-adding 'agent' tag (enables --jinja for tool calling)");
        tags.push("agent".to_string());
    }

    tags
}

/// Apply both reasoning and tool calling detection, returning combined tags.
///
/// This is the recommended function for model import flows as it handles
/// both capability detections in a single call and avoids duplicate tags.
///
/// # Arguments
/// * `metadata` - The processed metadata HashMap from GGUF parsing
///
/// # Returns
/// A `Vec<String>` of unique tags to add to the model
pub fn apply_capability_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let mut tags = Vec::new();

    // Apply reasoning detection
    let reasoning_tags = apply_reasoning_detection(metadata);
    for tag in reasoning_tags {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    // Apply tool calling detection
    let tool_tags = apply_tool_detection(metadata);
    for tag in tool_tags {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    tags
}

#[cfg(test)]
mod reasoning_detection_tests {
    use super::*;

    #[test]
    fn test_detect_think_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% if message.role == 'assistant' %}<think>{{ message.thinking }}</think>{% endif %}"
                .to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        assert!(detection.confidence >= 0.3);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("think"))
        );
        assert_eq!(detection.suggested_format, Some("deepseek".to_string()));
    }

    #[test]
    fn test_detect_reasoning_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<reasoning>thoughts here</reasoning>response".to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("reasoning"))
        );
    }

    #[test]
    fn test_detect_seed_think_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<seed:think>thinking</seed:think>response".to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
    }

    #[test]
    fn test_detect_command_r_style() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<|START_THINKING|>thoughts<|END_THINKING|>response".to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
    }

    #[test]
    fn test_detect_by_model_name() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "DeepSeek-R1-Distill-Qwen-32B".to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("deepseek-r1"))
        );
    }

    #[test]
    fn test_detect_qwen3_by_name() {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Qwen3-4B-Thinking".to_string());

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
    }

    #[test]
    fn test_no_detection_for_regular_model() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% for message in messages %}{{ message.content }}{% endfor %}".to_string(),
        );
        metadata.insert("general.name".to_string(), "Llama-2-7B-Chat".to_string());

        let detection = detect_reasoning_support(&metadata);
        assert!(!detection.supports_reasoning);
        assert!(detection.confidence < 0.3);
    }

    #[test]
    fn test_combined_detection() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "... <think> ... </think> ...".to_string(),
        );
        metadata.insert("general.name".to_string(), "DeepSeek-R1-Qwen".to_string());

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        // Should have high confidence from multiple sources
        assert!(detection.confidence >= 0.5);
    }

    #[test]
    fn test_is_reasoning_model_helper() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>test</think>".to_string(),
        );

        assert!(is_reasoning_model(&metadata));

        let empty_metadata = HashMap::new();
        assert!(!is_reasoning_model(&empty_metadata));
    }
}

#[cfg(test)]
mod tool_calling_detection_tests {
    use super::*;

    #[test]
    fn test_detect_hermes_tool_call_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"{% if tools %}<tool_call>{{ tool | tojson }}</tool_call>{% endif %}"#.to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(detection.confidence >= 0.3);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("tool_call"))
        );
        assert_eq!(detection.detected_format, Some("hermes".to_string()));
    }

    #[test]
    fn test_detect_mistral_tool_calling() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"[TOOL_CALLS] {{ tools | tojson }} [TOOL_RESULTS]"#.to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.to_lowercase().contains("tool_calls"))
        );
        assert_eq!(detection.detected_format, Some("mistral".to_string()));
    }

    #[test]
    fn test_detect_llama3_python_tag() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"<|python_tag|>{{ code }}"#.to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert_eq!(detection.detected_format, Some("llama3".to_string()));
    }

    #[test]
    fn test_detect_deepseek_tool_calling() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"<｜tool▁calls▁begin｜>{{ tool_call }}<｜tool▁call▁begin｜>"#.to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert_eq!(detection.detected_format, Some("deepseek".to_string()));
    }

    #[test]
    fn test_detect_jinja_tools_conditional() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"{% if tools is defined and tools | length > 0 %}Available tools: {{ tools }}{% endif %}"#.to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("jinja"))
        );
    }

    #[test]
    fn test_detect_by_model_name_hermes() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "NousResearch/Hermes-3-Llama-3.1-8B".to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("hermes"))
        );
    }

    #[test]
    fn test_detect_by_model_name_functionary() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "meetkai/functionary-small-v3.2".to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
    }

    #[test]
    fn test_no_detection_for_regular_model() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% for message in messages %}{{ message.content }}{% endfor %}".to_string(),
        );
        metadata.insert("general.name".to_string(), "Llama-2-7B-Chat".to_string());

        let detection = detect_tool_support(&metadata);
        assert!(!detection.supports_tool_calling);
        assert!(detection.confidence < 0.3);
    }

    #[test]
    fn test_is_tool_capable_model_helper() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );

        assert!(is_tool_capable_model(&metadata));

        let empty_metadata = HashMap::new();
        assert!(!is_tool_capable_model(&empty_metadata));
    }

    #[test]
    fn test_apply_tool_detection_adds_agent_tag() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );

        let tags = apply_tool_detection(&metadata);
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_combined_capability_detection() {
        let mut metadata = HashMap::new();
        // Model with both reasoning AND tool calling
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>reasoning</think><tool_call>tool</tool_call>".to_string(),
        );

        let tags = apply_capability_detection(&metadata);
        assert!(tags.contains(&"reasoning".to_string()));
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_capability_detection_no_duplicates() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>tool</tool_call>".to_string(),
        );

        let tags = apply_capability_detection(&metadata);
        // Should only have one "agent" tag
        assert_eq!(tags.iter().filter(|t| *t == "agent").count(), 1);
    }
}
