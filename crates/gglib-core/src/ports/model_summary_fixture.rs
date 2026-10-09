//! A [`ModelSummary`] made without a catalogue, for tests and catalogue
//! doubles.

use super::ModelSummary;
use crate::domain::ModelCapabilities;

impl ModelSummary {
    /// A model named `name` under `id` with nothing else known about it: no
    /// tags, no capabilities, no projector, no defaults. A test sets the
    /// fields it reads, so a new field is added here and nowhere else.
    #[must_use]
    pub fn bare(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_owned(),
            tags: Vec::new(),
            capabilities: ModelCapabilities::empty(),
            image_input: false,
            image_output: false,
            param_count: "7B".to_owned(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            dialect: None,
            template_caps: None,
        }
    }
}
