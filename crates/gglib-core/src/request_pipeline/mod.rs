#![doc = include_str!("README.md")]
pub mod model_context;
pub mod resolve;

pub use model_context::ModelContext;
pub use resolve::resolve;

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
            server_defaults: None,
        }
    }
}
