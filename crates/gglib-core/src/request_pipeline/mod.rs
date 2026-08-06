#![doc = include_str!("README.md")]
pub mod apply;
pub mod constrain;
pub mod messages;
pub mod model_context;
pub mod resolve;
pub mod sampling;
pub mod tools;
pub mod truncation;

pub use apply::apply;
pub use constrain::{DISABLE_GRAMMAR_ENV, constrain_tool_calls};
pub use messages::shape_messages;
pub use model_context::ModelContext;
pub use resolve::resolve;
pub use sampling::{SamplingLayers, resolve_sampling};
pub use tools::strip_unsupported_tools;
pub use truncation::{CHARS_PER_TOKEN_APPROX, TruncationError, TruncationReport, truncate_history};

#[cfg(test)]
mod tests_support {
    use crate::ports::ModelSummary;

    /// A minimal, inert [`ModelSummary`]. Tests set only the fields they care
    /// about, so adding a field to `ModelSummary` doesn't touch every test.
    pub fn summary() -> ModelSummary {
        ModelSummary {
            id: 7,
            name: "qwen3".to_string(),
            tags: Vec::new(),
            capabilities: crate::domain::ModelCapabilities::empty(),
            param_count: "7B".to_string(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }
    }
}
