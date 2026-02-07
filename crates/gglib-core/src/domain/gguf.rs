//! GGUF domain types.
//!
//! This module contains the domain-facing types for GGUF file metadata
//! and model capabilities. Parsing logic lives in `gglib-gguf`.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

// =============================================================================
// Capabilities (Structured, forward-compatible)
// =============================================================================

bitflags::bitflags! {
    /// Known model capabilities detected from GGUF metadata.
    ///
    /// Uses bitflags for compile-time safety on stable capabilities.
    /// Unknown/experimental capabilities go in `GgufCapabilities::extensions`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct CapabilityFlags: u32 {
        /// Model supports reasoning/thinking (e.g., DeepSeek R1, QwQ).
        const REASONING = 0b0000_0001;
        /// Model supports tool/function calling (e.g., Hermes, Functionary).
        const TOOL_CALLING = 0b0000_0010;
        /// Model supports vision/image input.
        const VISION = 0b0000_0100;
        /// Model supports code generation.
        const CODE = 0b0000_1000;
        /// Model is a mixture-of-experts architecture.
        const MOE = 0b0001_0000;
    }
}

/// Model capabilities detected from GGUF metadata.
///
/// Combines stable known capabilities (bitflags) with forward-compatible
/// extension strings for new/experimental capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GgufCapabilities {
    /// Known stable capabilities (compile-time checked).
    pub flags: CapabilityFlags,
    /// Unknown/experimental capabilities (forward-compatible).
    pub extensions: BTreeSet<String>,
}

impl GgufCapabilities {
    /// Create empty capabilities.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            flags: CapabilityFlags::empty(),
            extensions: BTreeSet::new(),
        }
    }

    /// Check if reasoning is supported.
    #[must_use]
    pub const fn has_reasoning(&self) -> bool {
        self.flags.contains(CapabilityFlags::REASONING)
    }

    /// Check if tool calling is supported.
    #[must_use]
    pub const fn has_tool_calling(&self) -> bool {
        self.flags.contains(CapabilityFlags::TOOL_CALLING)
    }

    /// Check if vision is supported.
    #[must_use]
    pub const fn has_vision(&self) -> bool {
        self.flags.contains(CapabilityFlags::VISION)
    }

    /// Convert capabilities to tag strings for model metadata.
    ///
    /// Returns tags like "reasoning", "agent" (for tool calling), etc.
    #[must_use]
    pub fn to_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();

        if self.has_reasoning() {
            tags.push("reasoning".to_string());
        }
        if self.has_tool_calling() {
            // "agent" tag triggers --jinja auto-enable
            tags.push("agent".to_string());
        }
        if self.has_vision() {
            tags.push("vision".to_string());
        }
        if self.flags.contains(CapabilityFlags::CODE) {
            tags.push("code".to_string());
        }
        if self.flags.contains(CapabilityFlags::MOE) {
            tags.push("moe".to_string());
        }

        // Add extension tags
        for ext in &self.extensions {
            if !tags.contains(ext) {
                tags.push(ext.clone());
            }
        }

        tags
    }
}

// =============================================================================
// Metadata value types
// =============================================================================

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
            Self::U8(v) => write!(f, "{v}"),
            Self::I8(v) => write!(f, "{v}"),
            Self::U16(v) => write!(f, "{v}"),
            Self::I16(v) => write!(f, "{v}"),
            Self::U32(v) => write!(f, "{v}"),
            Self::I32(v) => write!(f, "{v}"),
            Self::F32(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::String(v) => write!(f, "{v}"),
            Self::U64(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::F64(v) => write!(f, "{v}"),
            Self::Array(arr) => {
                // Limit array output to prevent massive tokenizer vocab dumps
                if arr.len() > 10 {
                    write!(f, "[Array with {} elements]", arr.len())
                } else {
                    write!(
                        f,
                        "[{}]",
                        arr.iter()
                            .map(std::string::ToString::to_string)
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
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U8(v) => Some(u64::from(*v)),
            Self::U16(v) => Some(u64::from(*v)),
            Self::U32(v) => Some(u64::from(*v)),
            Self::U64(v) => Some(*v),
            Self::I8(v) if *v >= 0 => Some(*v as u64),
            Self::I16(v) if *v >= 0 => Some(*v as u64),
            Self::I32(v) if *v >= 0 => Some(*v as u64),
            Self::I64(v) if *v >= 0 => Some(*v as u64),
            _ => None,
        }
    }

    /// Try to convert the value to a f64.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F32(v) => Some(f64::from(*v)),
            Self::F64(v) => Some(*v),
            Self::U8(v) => Some(f64::from(*v)),
            Self::U16(v) => Some(f64::from(*v)),
            Self::U32(v) => Some(f64::from(*v)),
            Self::U64(v) => Some(*v as f64),
            Self::I8(v) => Some(f64::from(*v)),
            Self::I16(v) => Some(f64::from(*v)),
            Self::I32(v) => Some(f64::from(*v)),
            Self::I64(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Try to get the value as a string reference.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

// =============================================================================
// Metadata
// =============================================================================

/// Parsed metadata from a GGUF file.
///
/// This is the domain-facing type used by services and ports.
/// Parsing logic that produces this type lives in `gglib-gguf`.
#[derive(Debug, Clone, Default)]
pub struct GgufMetadata {
    /// Model name from general.name metadata or filename.
    pub name: Option<String>,
    /// Model architecture (e.g., "llama", "mistral").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_K_M`", "`Q8_0`").
    pub quantization: Option<String>,
    /// Number of parameters in billions.
    pub param_count_b: Option<f64>,
    /// Maximum context length.
    pub context_length: Option<u64>,
    /// Number of experts (for MoE models).
    pub expert_count: Option<u32>,
    /// Number of experts used during inference (for MoE models).
    pub expert_used_count: Option<u32>,
    /// Number of shared experts (for MoE models).
    pub expert_shared_count: Option<u32>,
    /// Additional key-value metadata from the file (string representation).
    pub metadata: HashMap<String, String>,
}

/// Raw metadata from GGUF parsing (before string conversion).
///
/// Used internally by parsers; services typically use `GgufMetadata`.
pub type RawMetadata = HashMap<String, GgufValue>;

// =============================================================================
// Detection results (for detailed analysis)
// =============================================================================

/// Result of reasoning capability detection.
#[derive(Debug, Clone, Default)]
pub struct ReasoningDetection {
    /// Whether the model appears to support reasoning/thinking.
    pub supports_reasoning: bool,
    /// Confidence level of the detection (0.0 to 1.0).
    pub confidence: f32,
    /// The specific pattern(s) that matched.
    pub matched_patterns: Vec<String>,
    /// Suggested reasoning format for llama-server.
    pub suggested_format: Option<String>,
}

/// Result of tool calling capability detection.
#[derive(Debug, Clone, Default)]
pub struct ToolCallingDetection {
    /// Whether the model appears to support tool/function calling.
    pub supports_tool_calling: bool,
    /// Confidence level of the detection (0.0 to 1.0).
    pub confidence: f32,
    /// The specific pattern(s) that matched.
    pub matched_patterns: Vec<String>,
    /// Detected tool calling format (e.g., "hermes", "llama3", "mistral").
    pub detected_format: Option<String>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_empty() {
        let caps = GgufCapabilities::empty();
        assert!(!caps.has_reasoning());
        assert!(!caps.has_tool_calling());
        assert!(caps.to_tags().is_empty());
    }

    #[test]
    fn test_capabilities_flags() {
        let caps = GgufCapabilities {
            flags: CapabilityFlags::REASONING | CapabilityFlags::TOOL_CALLING,
            extensions: BTreeSet::new(),
        };
        assert!(caps.has_reasoning());
        assert!(caps.has_tool_calling());

        let tags = caps.to_tags();
        assert!(tags.contains(&"reasoning".to_string()));
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_capabilities_extensions() {
        let mut extensions = BTreeSet::new();
        extensions.insert("experimental-feature".to_string());

        let caps = GgufCapabilities {
            flags: CapabilityFlags::empty(),
            extensions,
        };

        let tags = caps.to_tags();
        assert!(tags.contains(&"experimental-feature".to_string()));
    }

    #[test]
    fn test_gguf_value_as_u64() {
        assert_eq!(GgufValue::U32(4096).as_u64(), Some(4096));
        assert_eq!(GgufValue::I32(-1).as_u64(), None);
        assert_eq!(GgufValue::String("hello".to_string()).as_u64(), None);
        assert_eq!(GgufValue::I32(100).as_u64(), Some(100));
    }

    #[test]
    fn test_gguf_value_as_f64() {
        assert!((GgufValue::F32(7.5).as_f64().unwrap() - 7.5).abs() < f64::EPSILON);
        assert!((GgufValue::U64(1000).as_f64().unwrap() - 1000.0).abs() < f64::EPSILON);
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
