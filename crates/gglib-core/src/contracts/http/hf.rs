//! Hugging Face API route constants.

/// Search/browse HF models endpoint.
pub const SEARCH_PATH: &str = "/api/models/hf/search";

/// Get available quantizations for a model.
/// Use with format!() to interpolate `model_id`.
pub const QUANTIZATIONS_PATH: &str = "/api/models/hf/quantizations";

/// Check tool support for a model.
/// Use with format!() to interpolate `model_id`.
pub const TOOL_SUPPORT_PATH: &str = "/api/models/hf/tool-support";
