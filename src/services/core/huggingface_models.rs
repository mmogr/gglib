//! Data models and error types for the HuggingFace service.
//!
//! This module contains error types for HuggingFace API operations,
//! following the pattern established by `download_models.rs` and
//! `database/error.rs`.

use thiserror::Error;

/// Errors related to HuggingFace API operations.
#[derive(Debug, Error)]
pub enum HuggingFaceError {
    /// API request failed with an HTTP error status.
    #[error("HuggingFace API request failed with status {status}: {url}")]
    ApiRequestFailed { status: u16, url: String },

    /// API returned an invalid or unexpected response.
    #[error("Invalid response from HuggingFace API: {message}")]
    InvalidResponse { message: String },

    /// The requested model was not found.
    #[error("Model '{model_id}' not found on HuggingFace")]
    ModelNotFound { model_id: String },

    /// Network or HTTP client error.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_request_failed_error_message() {
        let error = HuggingFaceError::ApiRequestFailed {
            status: 404,
            url: "https://huggingface.co/api/models/test".to_string(),
        };
        assert!(error.to_string().contains("404"));
        assert!(error.to_string().contains("huggingface.co"));
    }

    #[test]
    fn test_invalid_response_error_message() {
        let error = HuggingFaceError::InvalidResponse {
            message: "Missing required field 'id'".to_string(),
        };
        assert!(error.to_string().contains("Missing required field"));
    }

    #[test]
    fn test_model_not_found_error_message() {
        let error = HuggingFaceError::ModelNotFound {
            model_id: "TheBloke/NonExistent-GGUF".to_string(),
        };
        assert!(error.to_string().contains("TheBloke/NonExistent-GGUF"));
        assert!(error.to_string().contains("not found"));
    }
}
