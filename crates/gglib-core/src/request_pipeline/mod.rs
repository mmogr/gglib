#![doc = include_str!("README.md")]
pub mod apply;
pub(crate) mod constrain;
pub(crate) mod content;
pub(crate) mod effort_gate;
pub mod explain;
pub(crate) mod image_size;
pub(crate) mod images;
pub(crate) mod measure;
pub(crate) mod messages;
pub(crate) mod model_context;
pub mod profile_route;
pub(crate) mod request_shape;
pub mod resolve;
pub(crate) mod sampling;
pub(crate) mod sampling_log;
pub(crate) mod tools;
pub(crate) mod truncation;
pub(crate) mod truncation_parts;
pub mod validate;

pub use apply::{PipelineReport, apply};
pub use constrain::{DISABLE_GRAMMAR_ENV, constrain_tool_calls};
pub use content::{append_text, for_each_text_mut, image_urls, text_len, text_parts};
pub use effort_gate::{SuppressedEffort, suppress_stored_effort, suppress_unsupported_effort};
pub use explain::explain_stored;
pub use image_size::{
    JPEG_MIME, MAX_HEADER_BYTES, PNG_MIME, data_url_image_size, image_mime, image_size,
};
pub use images::{
    CannotReadImages, IMAGE_TOKEN_PX, MAX_IMAGE_BYTES, MAX_IMAGE_TOKENS, MAX_REQUEST_IMAGE_BYTES,
    estimate_image_tokens, has_images, image_url_tokens, refuse_unless_can_see, request_image_urls,
};
pub use measure::ContextBudget;
pub use messages::shape_messages;
pub use model_context::ModelContext;
pub use profile_route::{ModelRoute, resolve_route};
pub use request_shape::carries_tools;
pub use resolve::{resolve, resolve_summary};
pub use sampling::{
    CLIENT_AUTHORITATIVE_KEYS, DISABLE_AGENTIC_SAMPLING_ENV, FloorClass, LADDER_RUNGS,
    SamplingDecision, SamplingLayers, resolve_sampling,
};
pub use tools::strip_unsupported_tools;
pub use truncation::{CHARS_PER_TOKEN_APPROX, TruncationError, TruncationReport, truncate_history};
pub use validate::{Verdict, Violation, ViolationKind, validate_tool_calls};

#[cfg(test)]
pub(crate) mod image_fixtures;

#[cfg(test)]
mod tests_support {
    use crate::ports::ModelSummary;

    /// A minimal, inert [`ModelSummary`]. Tests set only the fields they care
    /// about, so adding a field to `ModelSummary` doesn't touch every test.
    pub(super) fn summary() -> ModelSummary {
        ModelSummary::bare(7, "qwen3")
    }

    /// A budget of `chars` characters at the static ratio, for a test that
    /// thinks in characters.
    pub(super) const fn chars(chars: usize) -> super::ContextBudget {
        super::ContextBudget {
            chars,
            tokens: chars / super::CHARS_PER_TOKEN_APPROX,
        }
    }
}
