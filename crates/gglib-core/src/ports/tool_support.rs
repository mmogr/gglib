//! Tool support detection port.
//!
//! This port defines the interface for detecting whether models support
//! tool/function calling capabilities based on their metadata.

/// The source/provider of the model being analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    /// Local GGUF file.
    LocalGguf,
    /// `HuggingFace` model.
    HuggingFace,
}

/// Input for tool support detection.
///
/// Contains metadata from various sources (chat templates, tags, model names)
/// that can be analyzed to determine tool calling support.
#[derive(Debug, Clone)]
pub struct ToolSupportDetectionInput<'a> {
    /// Model identifier (file path or HF model ID).
    pub model_id: &'a str,
    /// Chat template text (e.g., from `tokenizer.chat_template`).
    pub chat_template: Option<&'a str>,
    /// Model tags (e.g., from `HuggingFace`).
    pub tags: &'a [String],
    /// Source of the model.
    pub source: ModelSource,
}

/// Tool calling format detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolFormat {
    /// Hermes-style tool calling.
    Hermes,
    /// Llama 3.x style.
    Llama3,
    /// Mistral style.
    Mistral,
    /// `OpenAI` tools format.
    OpenAiTools,
    /// Generic/unknown format.
    Generic,
}

impl ToolFormat {
    /// Convert to string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Hermes => "hermes",
            Self::Llama3 => "llama3",
            Self::Mistral => "mistral",
            Self::OpenAiTools => "openai-tools",
            Self::Generic => "generic",
        }
    }
}

impl std::fmt::Display for ToolFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of tool support detection.
#[derive(Debug, Clone)]
pub struct ToolSupportDetection {
    /// Whether the model likely supports tool/function calling.
    pub supports_tool_calling: bool,
    /// Confidence level (0.0 = unknown/no support, 1.0 = certain support).
    pub confidence: f32,
    /// Detected tool calling format, if identified.
    pub detected_format: Option<ToolFormat>,
}

/// Port for detecting tool/function calling support in models.
///
/// Implementations analyze model metadata (chat templates, tags, names)
/// to determine if a model supports tool calling capabilities.
pub trait ToolSupportDetectorPort: Send + Sync {
    /// Detect tool support based on model metadata.
    fn detect(&self, input: ToolSupportDetectionInput<'_>) -> ToolSupportDetection;
}
