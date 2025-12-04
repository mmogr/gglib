//! GGUF type definitions.
//!
//! This module contains the core types used for representing GGUF file structure
//! and metadata.

use std::collections::HashMap;
use std::fmt;

/// GGUF metadata value types.
///
/// Represents all possible value types that can appear in GGUF metadata.
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
    /// Try to convert the value to a u64.
    ///
    /// Attempts to convert various numeric GGUF value types to u64.
    /// Only converts non-negative values to avoid overflow issues.
    ///
    /// # Returns
    /// * `Some(u64)` if the value can be safely converted
    /// * `None` if the value is negative, non-numeric, or would overflow
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

    /// Try to convert the value to a f64.
    ///
    /// Attempts to convert various numeric GGUF value types to f64.
    ///
    /// # Returns
    /// * `Some(f64)` if the value can be converted to a float
    /// * `None` if the value is non-numeric (String, Array, Bool)
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

    /// Try to get the value as a string reference.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            GgufValue::String(s) => Some(s),
            _ => None,
        }
    }
}

/// Metadata extracted from a GGUF file header.
///
/// This struct represents the parsed metadata from a GGUF file.
#[derive(Debug, Clone, Default)]
pub struct GgufMetadata {
    /// Model name from general.name metadata
    pub name: Option<String>,
    /// Model architecture from general.architecture
    pub architecture: Option<String>,
    /// Parameter count calculated from model structure
    pub param_count_b: Option<f64>,
    /// Quantization information derived from file_type or filename
    pub quantization: Option<String>,
    /// Context length from architecture-specific metadata
    pub context_length: Option<u64>,
    /// All metadata key-value pairs (string representation)
    pub metadata: HashMap<String, String>,
}

/// Raw metadata from GGUF parsing (before string conversion).
pub type RawMetadata = HashMap<String, GgufValue>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gguf_value_as_u64() {
        assert_eq!(GgufValue::U32(4096).as_u64(), Some(4096));
        assert_eq!(GgufValue::I32(-1).as_u64(), None);
        assert_eq!(GgufValue::String("hello".to_string()).as_u64(), None);
        assert_eq!(GgufValue::I32(100).as_u64(), Some(100));
    }

    #[test]
    fn test_gguf_value_as_f64() {
        assert_eq!(GgufValue::F32(7.5).as_f64(), Some(7.5));
        assert_eq!(GgufValue::U64(1000).as_f64(), Some(1000.0));
        assert_eq!(GgufValue::Bool(true).as_f64(), None);
    }

    #[test]
    fn test_gguf_value_display() {
        assert_eq!(GgufValue::U32(42).to_string(), "42");
        assert_eq!(GgufValue::String("test".to_string()).to_string(), "test");

        let large_array = GgufValue::Array(vec![GgufValue::U8(0); 100]);
        assert!(large_array.to_string().contains("100 elements"));
    }
}
